//! Windows desktop-layer adapter.
//!
//! WorkerW is Explorer-owned and undocumented. The adapter therefore verifies every
//! attach operation and restores the original top-level window style on failure.

use windows_sys::{
    core::BOOL,
    Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, FindWindowExW, FindWindowW, GetAncestor, GetClassNameW, GetWindowLongPtrW,
            SendMessageTimeoutW, SetWindowLongPtrW, SetWindowPos, GWLP_HWNDPARENT, GWL_EXSTYLE,
            HWND_NOTOPMOST, SMTO_NORMAL, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            WS_EX_NOACTIVATE,
        },
    },
};

const PROGMAN: &[u16] = &[80, 114, 111, 103, 109, 97, 110, 0];
const SHELL_DLL_DEF_VIEW: &[u16] = &[
    83, 72, 69, 76, 76, 68, 76, 76, 95, 68, 101, 102, 86, 105, 101, 119, 0,
];
const SPAWN_WORKER_W_MESSAGE: u32 = 0x052c;
const GA_ROOT: u32 = 2;

#[derive(Clone, Copy)]
pub struct DesktopAttachment {
    desktop_owner: usize,
    original_owner: isize,
    original_extended_style: isize,
    applied_extended_style: isize,
}

/// `WebviewWindow::hwnd` may point to a WebView child on some Windows builds.
/// All desktop parenting must target its native top-level Tauri window instead.
pub fn top_level_window(window_handle: HWND) -> HWND {
    let root = unsafe { GetAncestor(window_handle, GA_ROOT) };
    if root.is_null() {
        window_handle
    } else {
        root
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

pub fn attach(window_handle: HWND) -> Result<DesktopAttachment, String> {
    let desktop_owner = workerw()?;
    let original_owner = unsafe { GetWindowLongPtrW(window_handle, GWLP_HWNDPARENT) };
    let original_extended_style = unsafe { GetWindowLongPtrW(window_handle, GWL_EXSTYLE) };
    let applied_extended_style = original_extended_style | WS_EX_NOACTIVATE as isize;

    unsafe {
        // Keep the Tauri window top-level. Direct child reparenting to WorkerW
        // caused WebView rendering and input to be stranded below desktop icons
        // on this Windows build. An owner anchor preserves normal window input
        // while placing the window in the desktop host's z-order family.
        SetWindowLongPtrW(window_handle, GWLP_HWNDPARENT, desktop_owner as isize);
        SetWindowLongPtrW(window_handle, GWL_EXSTYLE, applied_extended_style);
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

    let actual_owner = unsafe { GetWindowLongPtrW(window_handle, GWLP_HWNDPARENT) };
    if actual_owner != desktop_owner as isize {
        unsafe {
            SetWindowLongPtrW(window_handle, GWLP_HWNDPARENT, original_owner);
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
            "桌面层锚定未生效（目标 0x{:X}，实际 0x{:X}）；已恢复普通窗口模式",
            desktop_owner as usize, actual_owner as usize
        ));
    }

    Ok(DesktopAttachment {
        desktop_owner: desktop_owner as usize,
        original_owner,
        original_extended_style,
        applied_extended_style,
    })
}

pub fn reapply(window_handle: HWND, attachment: DesktopAttachment) -> Result<(), String> {
    unsafe {
        SetWindowLongPtrW(
            window_handle,
            GWLP_HWNDPARENT,
            attachment.desktop_owner as isize,
        );
        SetWindowLongPtrW(
            window_handle,
            GWL_EXSTYLE,
            attachment.applied_extended_style,
        );
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

    let actual_owner = unsafe { GetWindowLongPtrW(window_handle, GWLP_HWNDPARENT) };
    if actual_owner != attachment.desktop_owner as isize {
        return Err("桌面模式恢复失败；窗口仍可从托盘切回普通模式".to_string());
    }
    Ok(())
}

pub fn detach(window_handle: HWND, attachment: DesktopAttachment) {
    unsafe {
        SetWindowLongPtrW(window_handle, GWLP_HWNDPARENT, attachment.original_owner);
        SetWindowLongPtrW(
            window_handle,
            GWL_EXSTYLE,
            attachment.original_extended_style,
        );
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
}
