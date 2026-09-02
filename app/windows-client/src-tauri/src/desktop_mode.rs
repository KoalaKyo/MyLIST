//! Windows desktop-layer adapter.
//!
//! WorkerW is Explorer-owned and undocumented. The adapter therefore verifies every
//! attach operation and restores the original top-level window style on failure.

use windows_sys::{
    core::BOOL,
    Win32::{
        Foundation::{HWND, LPARAM, RECT, S_OK},
        Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST},
        UI::{
            HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI},
            WindowsAndMessaging::{
                EnumWindows, FindWindowExW, FindWindowW, GetClassNameW, GetParent,
                GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, SendMessageTimeoutW,
                SetParent, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, GWLP_HWNDPARENT,
                GWL_EXSTYLE, GWL_STYLE, HWND_NOTOPMOST, HWND_TOP, SMTO_NORMAL, SWP_FRAMECHANGED,
                SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_CAPTION, WS_CHILD, WS_EX_NOACTIVATE,
                WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
            },
        },
    },
};

const PROGMAN: &[u16] = &[80, 114, 111, 103, 109, 97, 110, 0];
const SHELL_DLL_DEF_VIEW: &[u16] = &[
    83, 72, 69, 76, 76, 68, 76, 76, 95, 68, 101, 102, 86, 105, 101, 119, 0,
];
const SPAWN_WORKER_W_MESSAGE: u32 = 0x052c;
const EMPTY_TITLE: &[u16] = &[0];
const RDW_INVALIDATE: u32 = 0x0001;
const RDW_ERASE: u32 = 0x0004;
const RDW_ALLCHILDREN: u32 = 0x0080;
const RDW_UPDATENOW: u32 = 0x0100;
const RDW_FRAME: u32 = 0x0400;

#[link(name = "user32")]
unsafe extern "system" {
    fn RedrawWindow(
        window: HWND,
        update_rect: *const core::ffi::c_void,
        update_region: isize,
        flags: u32,
    ) -> i32;
}

#[derive(Clone, Copy)]
pub struct DesktopAttachment {
    desktop_parent: usize,
    original_parent: usize,
    original_owner: isize,
    original_dpi: u32,
    original_width: i32,
    original_height: i32,
    original_style: isize,
    original_extended_style: isize,
    applied_style: isize,
    applied_extended_style: isize,
}

/// `WebviewWindow::hwnd` may point to a WebView child on some Windows builds.
/// All desktop parenting must target its native top-level Tauri window instead.
pub fn top_level_window(window_handle: HWND) -> HWND {
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(window_handle, &mut process_id) };
    let mut current = window_handle;
    loop {
        let parent = unsafe { GetParent(current) };
        if parent.is_null() {
            return current;
        }
        let mut parent_process_id = 0u32;
        unsafe { GetWindowThreadProcessId(parent, &mut parent_process_id) };
        if parent_process_id != process_id {
            // WorkerW belongs to Explorer. Returning it here previously allowed
            // MyLIST's auto-hide code to move the entire Windows desktop.
            return current;
        }
        current = parent;
    }
}

unsafe extern "system" fn find_workerw(window: HWND, lparam: LPARAM) -> BOOL {
    if unsafe {
        FindWindowExW(
            window,
            core::ptr::null_mut(),
            SHELL_DLL_DEF_VIEW.as_ptr(),
            core::ptr::null(),
        )
    }
    .is_null()
    {
        return 1;
    }

    // Use the WorkerW which hosts the desktop icons themselves.  The previous
    // implementation selected the next WorkerW sibling, which is deliberately
    // below the icons and makes an interactive todo widget impossible to use.
    unsafe { *(lparam as *mut HWND) = window };
    0
}

unsafe extern "system" fn find_progman(window: HWND, lparam: LPARAM) -> BOOL {
    let mut class_name = [0u16; 64];
    let length = unsafe { GetClassNameW(window, class_name.as_mut_ptr(), class_name.len() as i32) };
    if length > 0 && class_name[..length as usize] == PROGMAN[..PROGMAN.len() - 1] {
        unsafe { *(lparam as *mut HWND) = window };
        return 0;
    }
    1
}

fn workerw() -> Result<HWND, String> {
    let mut progman = unsafe { FindWindowW(PROGMAN.as_ptr(), core::ptr::null()) };
    if progman.is_null() {
        unsafe {
            EnumWindows(Some(find_progman), &mut progman as *mut HWND as LPARAM);
        }
    }
    if progman.is_null() {
        return Err("未找到 Windows 桌面宿主（Progman）".to_string());
    }

    let mut message_result = 0usize;
    unsafe {
        SendMessageTimeoutW(
            progman,
            SPAWN_WORKER_W_MESSAGE,
            0,
            0,
            SMTO_NORMAL,
            1_000,
            &mut message_result,
        );
    }

    let mut worker: HWND = core::ptr::null_mut();
    unsafe {
        EnumWindows(Some(find_workerw), &mut worker as *mut HWND as LPARAM);
    }
    if worker.is_null() {
        return Err("Windows 桌面层不可用；已保留普通窗口模式".to_string());
    }
    Ok(worker)
}

/// A transparent WebView can expose pixels cached before the Tauri window was
/// converted from a top-level window into a WorkerW child. Both surfaces must
/// be invalidated; repainting only the WebView leaves the old native frame.
pub fn refresh_window_surface(window_handle: HWND) {
    let desktop_parent = unsafe { GetParent(window_handle) };
    unsafe {
        if !desktop_parent.is_null() {
            RedrawWindow(
                desktop_parent,
                core::ptr::null(),
                0,
                RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW,
            );
        }
        RedrawWindow(
            window_handle,
            core::ptr::null(),
            0,
            RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN | RDW_UPDATENOW,
        );
        SetWindowTextW(window_handle, EMPTY_TITLE.as_ptr());
    }
}

pub fn target_monitor_dpi(window_handle: HWND) -> u32 {
    let monitor = unsafe { MonitorFromWindow(window_handle, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return unsafe { GetDpiForWindow(window_handle) }.max(96);
    }
    let mut dpi_x = 96u32;
    let mut dpi_y = 96u32;
    if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) } == S_OK {
        dpi_x.max(96)
    } else {
        unsafe { GetDpiForWindow(window_handle) }.max(96)
    }
}

impl DesktopAttachment {
    pub fn original_logical_size(self) -> Option<(f64, f64)> {
        if self.original_width <= 0 || self.original_height <= 0 {
            return None;
        }
        Some((
            self.original_width as f64 * 96.0 / self.original_dpi as f64,
            self.original_height as f64 * 96.0 / self.original_dpi as f64,
        ))
    }
}

/// Keeps a WorkerW-hosted window at the physical size appropriate for the
/// monitor under it and returns the WebView zoom needed to offset WorkerW's DPI.
pub fn adapt_to_monitor(window_handle: HWND, attachment: DesktopAttachment) -> f64 {
    let target_dpi = target_monitor_dpi(window_handle);
    // A WorkerW child can report the target monitor DPI even though WebView2
    // continues rasterizing at its Explorer-owned desktop host's DPI. Read the
    // host monitor directly so the visual compensation reflects what WebView2
    // actually renders.
    let host_dpi = unsafe { GetDpiForWindow(attachment.desktop_parent as HWND) }.max(96);
    if attachment.original_width > 0 && attachment.original_height > 0 {
        let target_width = ((attachment.original_width as i64 * target_dpi as i64
            + attachment.original_dpi as i64 / 2)
            / attachment.original_dpi as i64) as i32;
        let target_height = ((attachment.original_height as i64 * target_dpi as i64
            + attachment.original_dpi as i64 / 2)
            / attachment.original_dpi as i64) as i32;
        let mut current_rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if unsafe { GetWindowRect(window_handle, &mut current_rect) } != 0
            && (current_rect.right - current_rect.left != target_width
                || current_rect.bottom - current_rect.top != target_height)
        {
            unsafe {
                SetWindowPos(
                    window_handle,
                    HWND_TOP,
                    0,
                    0,
                    target_width,
                    target_height,
                    SWP_NOMOVE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
        }
    }
    target_dpi as f64 / host_dpi as f64
}

pub fn attach(window_handle: HWND) -> Result<DesktopAttachment, String> {
    let desktop_parent = workerw()?;
    let original_dpi = unsafe { GetDpiForWindow(window_handle) }.max(96);
    let mut original_rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let has_original_rect = unsafe { GetWindowRect(window_handle, &mut original_rect) } != 0;
    let original_parent = unsafe { GetParent(window_handle) };
    let original_owner = unsafe { GetWindowLongPtrW(window_handle, GWLP_HWNDPARENT) };
    let original_style = unsafe { GetWindowLongPtrW(window_handle, GWL_STYLE) };
    let original_extended_style = unsafe { GetWindowLongPtrW(window_handle, GWL_EXSTYLE) };
    let decoration_bits = (WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as isize;
    let applied_style =
        (original_style | WS_CHILD as isize) & !((WS_POPUP as isize) | decoration_bits);
    let applied_extended_style = original_extended_style | WS_EX_NOACTIVATE as isize;

    unsafe {
        SetWindowLongPtrW(window_handle, GWL_STYLE, applied_style);
        SetWindowLongPtrW(window_handle, GWL_EXSTYLE, applied_extended_style);
        SetWindowTextW(window_handle, EMPTY_TITLE.as_ptr());
        SetParent(window_handle, desktop_parent);
        SetWindowPos(
            window_handle,
            HWND_TOP,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }

    let actual_parent = unsafe { GetParent(window_handle) };
    if actual_parent != desktop_parent {
        unsafe {
            SetParent(window_handle, original_parent);
            SetWindowLongPtrW(window_handle, GWLP_HWNDPARENT, original_owner);
            SetWindowLongPtrW(window_handle, GWL_STYLE, original_style);
            SetWindowLongPtrW(window_handle, GWL_EXSTYLE, original_extended_style);
            SetWindowPos(
                window_handle,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
        return Err(format!(
            "桌面层绑定未生效（目标 0x{:X}，实际 0x{:X}）；已恢复普通窗口模式",
            desktop_parent as usize, actual_parent as usize
        ));
    }

    refresh_window_surface(window_handle);

    Ok(DesktopAttachment {
        desktop_parent: desktop_parent as usize,
        original_parent: original_parent as usize,
        original_owner,
        original_dpi,
        original_width: if has_original_rect {
            original_rect.right - original_rect.left
        } else {
            0
        },
        original_height: if has_original_rect {
            original_rect.bottom - original_rect.top
        } else {
            0
        },
        original_style,
        original_extended_style,
        applied_style,
        applied_extended_style,
    })
}

pub fn reapply(window_handle: HWND, attachment: DesktopAttachment) -> Result<(), String> {
    unsafe {
        SetWindowLongPtrW(window_handle, GWL_STYLE, attachment.applied_style);
        SetWindowLongPtrW(
            window_handle,
            GWL_EXSTYLE,
            attachment.applied_extended_style,
        );
        SetWindowTextW(window_handle, EMPTY_TITLE.as_ptr());
        SetParent(window_handle, attachment.desktop_parent as HWND);
        SetWindowPos(
            window_handle,
            HWND_TOP,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }

    let actual_parent = unsafe { GetParent(window_handle) };
    if actual_parent != attachment.desktop_parent as HWND {
        return Err("桌面模式恢复失败；窗口仍可从托盘切回普通模式".to_string());
    }
    refresh_window_surface(window_handle);
    Ok(())
}

pub fn detach(window_handle: HWND, attachment: DesktopAttachment) {
    unsafe {
        SetParent(window_handle, attachment.original_parent as HWND);
        SetWindowLongPtrW(window_handle, GWLP_HWNDPARENT, attachment.original_owner);
        SetWindowLongPtrW(window_handle, GWL_STYLE, attachment.original_style);
        SetWindowLongPtrW(
            window_handle,
            GWL_EXSTYLE,
            attachment.original_extended_style,
        );
        SetWindowTextW(window_handle, EMPTY_TITLE.as_ptr());
        SetWindowPos(
            window_handle,
            HWND_NOTOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }

    let restored_dpi = unsafe { GetDpiForWindow(window_handle) }.max(96);
    if attachment.original_width > 0 && attachment.original_height > 0 {
        let restored_width = ((attachment.original_width as i64 * restored_dpi as i64
            + attachment.original_dpi as i64 / 2)
            / attachment.original_dpi as i64) as i32;
        let restored_height = ((attachment.original_height as i64 * restored_dpi as i64
            + attachment.original_dpi as i64 / 2)
            / attachment.original_dpi as i64) as i32;
        unsafe {
            SetWindowPos(
                window_handle,
                HWND_NOTOPMOST,
                0,
                0,
                restored_width,
                restored_height,
                SWP_NOMOVE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
}
