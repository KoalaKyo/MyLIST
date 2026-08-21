//! Windows 10-compatible native window shaping.
//!
//! Windows 10 does not implement DWM's rounded-corner preference. Applying a
//! rounded HWND region gives the compositor the real window silhouette instead
//! of a rectangular transparent WebView host, so its frame cannot leak through
//! the four corners.

use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn},
    UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::GetWindowRect},
};

const CORNER_RADIUS_DIP: u32 = 8;

pub fn apply_rounded_region(window_handle: HWND) -> Result<(), String> {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if unsafe { GetWindowRect(window_handle, &mut rect) } == 0 {
        return Err("无法读取原生窗口尺寸".to_string());
    }

    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 {
        return Ok(());
    }

    let dpi = unsafe { GetDpiForWindow(window_handle) };
    let radius = ((CORNER_RADIUS_DIP * dpi.max(96)) / 96) as i32;
    let region = unsafe { CreateRoundRectRgn(0, 0, width + 1, height + 1, radius * 2, radius * 2) };
    if region.is_null() {
        return Err("无法创建窗口圆角区域".to_string());
    }

    // On success Windows owns the GDI region. On failure it remains ours.
    if unsafe { SetWindowRgn(window_handle, region, 1) } == 0 {
        unsafe { DeleteObject(region as _) };
        return Err("无法应用窗口圆角区域".to_string());
    }
    Ok(())
}
