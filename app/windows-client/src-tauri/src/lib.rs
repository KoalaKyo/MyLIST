#[cfg(target_os = "windows")]
mod auto_hide;
mod crypto;
mod data;
#[cfg(target_os = "windows")]
mod desktop_mode;
mod mcp;
mod mcp_bridge;
mod mcp_confirmation;
mod mcp_service;
mod mcp_stdio_bridge;
mod mcp_transfer;
mod native_i18n;
#[cfg(target_os = "windows")]
mod window_shape;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use data::DataStore;
use native_i18n::NativeLabels;

use tauri_plugin_autostart::ManagerExt as AutoStartManagerExt;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent,
};

pub fn run_mcp_bridge() -> std::io::Result<()> {
    mcp_stdio_bridge::run()
}

const MAIN_WINDOW: &str = "main";
const TOPMOST_MODE: &str = "mode-topmost";
const NORMAL_MODE: &str = "mode-normal";
const DESKTOP_MODE: &str = "mode-desktop";
const OPEN_MAIN: &str = "open-main";
const QUIT: &str = "quit";
#[cfg(target_os = "windows")]
const DEFAULT_WINDOW_WIDTH: u32 = 350;
#[cfg(target_os = "windows")]
const DEFAULT_WINDOW_HEIGHT: u32 = 530;

/// Repairs a saved geometry that no longer intersects any connected display.
/// This is especially important after a monitor is removed or after an older
/// build accidentally persisted an auto-hidden/off-screen rectangle.
#[cfg(target_os = "windows")]
fn safe_window_position(
    window: &tauri::WebviewWindow,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> (i32, i32) {
    let required_width = (width.min(64)) as i32;
    let required_height = (height.min(64)) as i32;
    let intersects_monitor = window.available_monitors().ok().is_some_and(|monitors| {
        monitors.into_iter().any(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            let monitor_right = position.x.saturating_add(size.width as i32);
            let monitor_bottom = position.y.saturating_add(size.height as i32);
            let overlap_width =
                (x.saturating_add(width as i32).min(monitor_right) - x.max(position.x)).max(0);
            let overlap_height =
                (y.saturating_add(height as i32).min(monitor_bottom) - y.max(position.y)).max(0);
            overlap_width >= required_width && overlap_height >= required_height
        })
    });
    if intersects_monitor {
        return (x, y);
    }

    window
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            (
                position.x + ((size.width as i32 - width as i32).max(0) / 2),
                position.y + ((size.height as i32 - height as i32).max(0) / 2),
            )
        })
        .unwrap_or((100, 100))
}

#[cfg(target_os = "windows")]
fn saved_size_fits_one_monitor(window: &tauri::WebviewWindow, width: u32, height: u32) -> bool {
    window
        .available_monitors()
        .ok()
        .map(|monitors| {
            monitors.into_iter().any(|monitor| {
                let size = monitor.size();
                width <= size.width && height <= size.height
            })
        })
        // A temporary monitor-enumeration failure must not discard an otherwise
        // valid saved geometry.
        .unwrap_or(true)
}

#[cfg(target_os = "windows")]
fn restore_window_geometry(window: &tauri::WebviewWindow, saved: &data::WindowStateDto) {
    let geometry_is_valid = saved_size_fits_one_monitor(window, saved.width, saved.height);
    let (width, height) = if geometry_is_valid {
        (
            saved.width.max(DEFAULT_WINDOW_WIDTH),
            saved.height.max(DEFAULT_WINDOW_HEIGHT),
        )
    } else {
        (DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT)
    };
    let (candidate_x, candidate_y) = if geometry_is_valid {
        (saved.x, saved.y)
    } else {
        // Force `safe_window_position` to choose a visible primary-monitor
        // position instead of reusing a corrupted multi-monitor rectangle.
        (i32::MAX, i32::MAX)
    };
    let (x, y) = safe_window_position(window, candidate_x, candidate_y, width, height);
    // If the saved monitor disappeared, keep Tauri's current/configured size
    // and repair only the position. This avoids turning a valid 350×530 DIP
    // window into a smaller physical-pixel rectangle during recovery.
    if geometry_is_valid && (x, y) == (saved.x, saved.y) {
        let _ = window.set_size(tauri::PhysicalSize::new(width, height));
    } else if !geometry_is_valid {
        let _ = window.set_size(tauri::PhysicalSize::new(width, height));
        if let Some(store) = window.app_handle().try_state::<DataStore>() {
            let _ = store.save_window_state(&data::WindowStateDto {
                x,
                y,
                width,
                height,
                mode: saved.mode.clone(),
            });
        }
    }
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    ensure_minimum_window_size(window);
}

#[cfg(target_os = "windows")]
fn ensure_minimum_window_size(window: &tauri::WebviewWindow) {
    const MIN_WIDTH_DIP: f64 = DEFAULT_WINDOW_WIDTH as f64;
    const MIN_HEIGHT_DIP: f64 = DEFAULT_WINDOW_HEIGHT as f64;
    let scale = window.scale_factor().unwrap_or(1.0).max(1.0);
    let minimum_width = (MIN_WIDTH_DIP * scale).ceil() as u32;
    let minimum_height = (MIN_HEIGHT_DIP * scale).ceil() as u32;
    let too_small = window
        .outer_size()
        .ok()
        .is_some_and(|size| size.width < minimum_width || size.height < minimum_height);
    if too_small {
        let _ = window.set_size(tauri::LogicalSize::new(MIN_WIDTH_DIP, MIN_HEIGHT_DIP));
    }
}

#[cfg(target_os = "windows")]
fn show_startup_data_error(message: &str) {
    use std::iter::once;
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

    let text: Vec<u16> = message.encode_utf16().chain(once(0)).collect();
    let title: Vec<u16> = "MyLIST".encode_utf16().chain(once(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_ICONERROR | MB_OK,
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowMode {
    Topmost,
    Normal,
    Desktop,
}

impl WindowMode {
    fn id(self) -> &'static str {
        match self {
            Self::Topmost => TOPMOST_MODE,
            Self::Normal => NORMAL_MODE,
            Self::Desktop => DESKTOP_MODE,
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        match id {
            TOPMOST_MODE => Some(Self::Topmost),
            NORMAL_MODE => Some(Self::Normal),
            DESKTOP_MODE => Some(Self::Desktop),
            _ => None,
        }
    }
}

struct WindowModeData {
    mode: WindowMode,
    #[cfg(target_os = "windows")]
    desktop_attachment: Option<desktop_mode::DesktopAttachment>,
    #[cfg(target_os = "windows")]
    normal_size_dip: Option<(f64, f64)>,
    #[cfg(target_os = "windows")]
    user_resizing: bool,
}

struct WindowModeState {
    data: Mutex<WindowModeData>,
}

impl Default for WindowModeState {
    fn default() -> Self {
        Self {
            data: Mutex::new(WindowModeData {
                mode: WindowMode::Normal,
                #[cfg(target_os = "windows")]
                desktop_attachment: None,
                #[cfg(target_os = "windows")]
                normal_size_dip: None,
                #[cfg(target_os = "windows")]
                user_resizing: false,
            }),
        }
    }
}

struct TrayModeControls {
    open_main: MenuItem<tauri::Wry>,
    topmost: CheckMenuItem<tauri::Wry>,
    normal: CheckMenuItem<tauri::Wry>,
    desktop: CheckMenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

enum PendingImport {
    Ready {
        id: String,
        package: data::PlaintextExportDto,
        source_file_name: String,
        operation: String,
    },
    Encrypted {
        id: String,
        encoded: Vec<u8>,
        source_file_name: String,
        operation: String,
    },
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportSelectionDto {
    kind: String,
    session_id: String,
    source_file_name: String,
    operation: String,
    preview: Option<data::ImportPreviewDto>,
}

#[derive(Default)]
struct PendingImportState {
    pending: Mutex<Option<PendingImport>>,
}

#[derive(Default)]
struct TrayModeControlsState {
    controls: Mutex<Option<TrayModeControls>>,
}

#[derive(Default)]
struct NativeLabelsState {
    labels: Mutex<NativeLabels>,
}

#[tauri::command]
fn approve_mcp_confirmation(
    state: tauri::State<'_, mcp_confirmation::McpConfirmationState>,
    token: String,
) -> Result<(), String> {
    state
        .approve(&token)
        .map_err(|error| mcp::error(error, false).code.to_string())
}

#[tauri::command]
fn reject_mcp_confirmation(
    state: tauri::State<'_, mcp_confirmation::McpConfirmationState>,
    token: String,
) -> Result<(), String> {
    state
        .reject(&token)
        .map_err(|error| mcp::error(error, false).code.to_string())
}

fn native_labels(app: &AppHandle) -> NativeLabels {
    app.state::<NativeLabelsState>()
        .labels
        .lock()
        .map(|labels| labels.clone())
        .unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn suspend_desktop_binding(app: &AppHandle, window: &tauri::WebviewWindow) -> Result<(), String> {
    let native_handle =
        desktop_mode::top_level_window(window.hwnd().map_err(|error| error.to_string())?.0);
    let attachment = {
        let state = app.state::<WindowModeState>();
        let mut data = state
            .data
            .lock()
            .map_err(|_| "窗口模式状态不可用".to_string())?;
        data.desktop_attachment.take()
    };
    if let Some(attachment) = attachment {
        desktop_mode::detach(native_handle, attachment);
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn resume_desktop_binding(app: &AppHandle, window: &tauri::WebviewWindow) -> Result<(), String> {
    let native_handle =
        desktop_mode::top_level_window(window.hwnd().map_err(|error| error.to_string())?.0);
    let attachment = {
        let state = app.state::<WindowModeState>();
        let attachment = state
            .data
            .lock()
            .map_err(|_| "窗口模式状态不可用".to_string())?
            .desktop_attachment;
        attachment
    };
    if let Some(attachment) = attachment {
        desktop_mode::reapply(native_handle, attachment)?;
        return Ok(());
    }

    let attachment = desktop_mode::attach(native_handle)?;
    let state = app.state::<WindowModeState>();
    state
        .data
        .lock()
        .map_err(|_| "窗口模式状态不可用".to_string())?
        .desktop_attachment = Some(attachment);
    Ok(())
}

fn restore_and_focus(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        #[cfg(target_os = "windows")]
        if let Ok(handle) = window.hwnd() {
            auto_hide::cancel_and_restore(app, handle.0);
            if let Ok(position) = window.outer_position() {
                if let Ok(size) = window.outer_size() {
                    let (x, y) = safe_window_position(
                        &window,
                        position.x,
                        position.y,
                        size.width,
                        size.height,
                    );
                    if (x, y) != (position.x, position.y) {
                        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
                    }
                }
            }
            ensure_minimum_window_size(&window);
        }
        window.show()?;
        window.unminimize()?;
        window.set_focus()?;
    }
    Ok(())
}

fn restore_for_current_mode(app: &AppHandle) -> tauri::Result<()> {
    let mode = read_window_mode(app).unwrap_or(WindowMode::Normal);
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return Ok(());
    };

    #[cfg(target_os = "windows")]
    if let Ok(handle) = window.hwnd() {
        auto_hide::cancel_and_restore(app, handle.0);
    }

    #[cfg(target_os = "windows")]
    if mode == WindowMode::Desktop {
        resume_desktop_binding(app, &window)
            .map_err(|error| tauri::Error::Anyhow(std::io::Error::other(error).into()))?;
    }

    window.show()?;
    window.unminimize()?;

    if mode != WindowMode::Desktop {
        return window.set_focus();
    }
    Ok(())
}

/// In desktop mode the primary tray action follows the Windows "Show desktop"
/// convention (Win+D), so the desktop-bound widget remains discoverable without
/// pulling it in front of the user's ordinary windows.
fn open_main_from_tray(app: &AppHandle) -> tauri::Result<()> {
    if read_window_mode(app).unwrap_or(WindowMode::Normal) == WindowMode::Desktop {
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
                keybd_event, KEYEVENTF_KEYUP, VK_D, VK_LWIN,
            };

            unsafe {
                keybd_event(VK_LWIN as u8, 0, 0, 0);
                keybd_event(VK_D as u8, 0, 0, 0);
                keybd_event(VK_D as u8, 0, KEYEVENTF_KEYUP, 0);
                keybd_event(VK_LWIN as u8, 0, KEYEVENTF_KEYUP, 0);
            }
            return Ok(());
        }
        #[cfg(not(target_os = "windows"))]
        return Ok(());
    }
    restore_and_focus(app)
}

fn read_window_mode(app: &AppHandle) -> Result<WindowMode, String> {
    let state = app.state::<WindowModeState>();
    let data = state
        .data
        .lock()
        .map_err(|_| "窗口模式状态不可用".to_string())?;
    Ok(data.mode)
}

#[cfg(target_os = "windows")]
fn read_desktop_attachment(app: &AppHandle) -> Option<desktop_mode::DesktopAttachment> {
    app.state::<WindowModeState>()
        .data
        .lock()
        .ok()
        .and_then(|data| data.desktop_attachment)
}

fn persist_window_state_for(
    app: &AppHandle,
    position: Option<(i32, i32)>,
    size: Option<(u32, u32)>,
) {
    #[cfg(target_os = "windows")]
    if auto_hide::is_collapsed_or_animating(app) {
        return;
    }
    let Some(store) = app.try_state::<DataStore>() else {
        return;
    };
    let (Some((x, y)), Some((width, height))) = (position, size) else {
        return;
    };
    // Never overwrite a recoverable geometry with a stale monitor coordinate.
    // Valid multi-monitor layouts can use negative coordinates, but an extreme
    // value indicates the old off-screen/auto-hide persistence bug.
    if x < -10_000 || y < -10_000 || x > 100_000 || y > 100_000 {
        return;
    }
    let mode = read_window_mode(app)
        .unwrap_or(WindowMode::Normal)
        .id()
        .to_string();
    if mode == DESKTOP_MODE {
        // A WorkerW child can temporarily report its Explorer host geometry.
        // Preserve the last known top-level rectangle and update only the mode;
        // never allow desktop-host dimensions to enter persisted app state.
        let existing = store.window_state().ok().flatten();
        let _ = store.save_window_state(&data::WindowStateDto {
            x: existing.as_ref().map(|state| state.x).unwrap_or(100),
            y: existing.as_ref().map(|state| state.y).unwrap_or(100),
            width: existing
                .as_ref()
                .map(|state| state.width.max(DEFAULT_WINDOW_WIDTH))
                .unwrap_or(DEFAULT_WINDOW_WIDTH),
            height: existing
                .as_ref()
                .map(|state| state.height.max(DEFAULT_WINDOW_HEIGHT))
                .unwrap_or(DEFAULT_WINDOW_HEIGHT),
            mode,
        });
        return;
    }
    let _ = store.save_window_state(&data::WindowStateDto {
        x,
        y,
        width,
        height,
        mode,
    });
}

fn persist_webview_window_state(window: &tauri::WebviewWindow) {
    persist_window_state_for(
        window.app_handle(),
        window.outer_position().ok().map(|value| (value.x, value.y)),
        window
            .outer_size()
            .ok()
            .map(|value| (value.width, value.height)),
    );
}

fn persist_window_state(window: &tauri::Window) {
    persist_window_state_for(
        window.app_handle(),
        window.outer_position().ok().map(|value| (value.x, value.y)),
        window
            .outer_size()
            .ok()
            .map(|value| (value.width, value.height)),
    );
}

#[cfg(target_os = "windows")]
fn normalize_tracked_window_size(
    window: &tauri::Window,
    native_handle: windows_sys::Win32::Foundation::HWND,
) {
    let logical_size = window
        .app_handle()
        .state::<WindowModeState>()
        .data
        .lock()
        .ok()
        .and_then(|data| data.normal_size_dip);
    let Some((logical_width, logical_height)) = logical_size else {
        return;
    };
    let dpi = desktop_mode::target_monitor_dpi(native_handle);
    let target_width =
        (logical_width.max(DEFAULT_WINDOW_WIDTH as f64) * dpi as f64 / 96.0).round() as u32;
    let target_height =
        (logical_height.max(DEFAULT_WINDOW_HEIGHT as f64) * dpi as f64 / 96.0).round() as u32;
    if window.outer_size().ok().is_some_and(|size| {
        size.width.abs_diff(target_width) > 1 || size.height.abs_diff(target_height) > 1
    }) {
        let _ = window.set_size(tauri::PhysicalSize::new(target_width, target_height));
    }
}

#[cfg(target_os = "windows")]
fn update_tracked_window_size(
    window: &tauri::Window,
    native_handle: windows_sys::Win32::Foundation::HWND,
) {
    let mode = read_window_mode(window.app_handle()).unwrap_or(WindowMode::Normal);
    if mode == WindowMode::Desktop {
        return;
    }
    let tracking_enabled = window
        .app_handle()
        .state::<WindowModeState>()
        .data
        .lock()
        .map(|data| data.normal_size_dip.is_some() && data.user_resizing)
        .unwrap_or(false);
    if !tracking_enabled {
        return;
    }
    let Ok(size) = window.outer_size() else {
        return;
    };
    let dpi = desktop_mode::target_monitor_dpi(native_handle).max(96);
    if let Ok(mut data) = window.app_handle().state::<WindowModeState>().data.lock() {
        data.normal_size_dip = Some((
            (size.width as f64 * 96.0 / dpi as f64).max(DEFAULT_WINDOW_WIDTH as f64),
            (size.height as f64 * 96.0 / dpi as f64).max(DEFAULT_WINDOW_HEIGHT as f64),
        ));
    }
}

fn synchronize_tray_checks(app: &AppHandle, mode: WindowMode) {
    let controls_state = app.state::<TrayModeControlsState>();
    let controls = match controls_state.controls.lock() {
        Ok(controls) => controls,
        Err(_) => return,
    };
    let Some(controls) = controls.as_ref() else {
        return;
    };
    let labels = native_labels(app);
    let _ = controls.open_main.set_text(if mode == WindowMode::Desktop {
        labels.show_desktop.clone()
    } else {
        labels.open_main.clone()
    });
    let _ = controls.topmost.set_text(labels.topmost.clone());
    let _ = controls.normal.set_text(labels.normal.clone());
    let _ = controls.desktop.set_text(labels.desktop);
    let _ = controls.quit.set_text(labels.quit);
    let _ = controls.topmost.set_checked(mode == WindowMode::Topmost);
    let _ = controls.normal.set_checked(mode == WindowMode::Normal);
    let _ = controls.desktop.set_checked(mode == WindowMode::Desktop);
}

fn publish_window_mode(app: &AppHandle, mode: WindowMode) {
    synchronize_tray_checks(app, mode);
    let _ = app.emit("window-mode-changed", mode.id());
}

fn apply_window_mode(app: &AppHandle, target_mode: WindowMode) -> Result<WindowMode, String> {
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or_else(|| "主窗口不可用".to_string())?;

    #[cfg(target_os = "windows")]
    let native_handle =
        desktop_mode::top_level_window(window.hwnd().map_err(|error| error.to_string())?.0);

    #[cfg(target_os = "windows")]
    auto_hide::cancel_and_restore(app, native_handle);

    let state = app.state::<WindowModeState>();
    let (previous_mode, desktop_attachment) = {
        let mut data = state
            .data
            .lock()
            .map_err(|_| "窗口模式状态不可用".to_string())?;
        if data.mode == target_mode {
            #[cfg(target_os = "windows")]
            auto_hide::recheck_after_mode_change(
                app.clone(),
                native_handle,
                target_mode != WindowMode::Desktop,
            );
            return Ok(target_mode);
        }
        (data.mode, data.desktop_attachment.take())
    };

    let was_visible = window.is_visible().unwrap_or(true);

    #[cfg(target_os = "windows")]
    if let Some(attachment) = desktop_attachment {
        let _ = window.hide();
        desktop_mode::detach(native_handle, attachment);
    }

    #[cfg(target_os = "windows")]
    let mut next_desktop_attachment = None;
    #[cfg(target_os = "windows")]
    let mut desktop_display_scale = None;

    match target_mode {
        WindowMode::Topmost => {
            window
                .set_always_on_top(true)
                .map_err(|error| error.to_string())?;
        }
        WindowMode::Normal => {
            if previous_mode == WindowMode::Topmost {
                window
                    .set_always_on_top(false)
                    .map_err(|error| error.to_string())?;
            }
        }
        WindowMode::Desktop => {
            if previous_mode == WindowMode::Topmost {
                window
                    .set_always_on_top(false)
                    .map_err(|error| error.to_string())?;
            }
            #[cfg(target_os = "windows")]
            {
                match desktop_mode::attach(native_handle) {
                    Ok(attachment) => {
                        desktop_display_scale =
                            Some(desktop_mode::adapt_to_monitor(native_handle, attachment));
                        next_desktop_attachment = Some(attachment);
                    }
                    Err(error) => {
                        let _ = window.show();
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                        return Err(error);
                    }
                }
            }
            #[cfg(not(target_os = "windows"))]
            return Err("桌面模式仅支持 Windows".to_string());
        }
    }

    {
        let mut data = state
            .data
            .lock()
            .map_err(|_| "窗口模式状态不可用".to_string())?;
        data.mode = target_mode;
        #[cfg(target_os = "windows")]
        {
            if let Some(attachment) = next_desktop_attachment {
                data.normal_size_dip = attachment.original_logical_size();
            }
            data.desktop_attachment = next_desktop_attachment;
        }
    }
    if was_visible && target_mode != WindowMode::Desktop {
        let _ = window.show();
        let _ = window.unminimize();
    }
    #[cfg(target_os = "windows")]
    if target_mode != WindowMode::Desktop {
        let _ = window_shape::disable_maximization(native_handle);
        let _ = window_shape::apply_rounded_region(native_handle);
        ensure_minimum_window_size(&window);
    }
    #[cfg(target_os = "windows")]
    auto_hide::recheck_after_mode_change(
        app.clone(),
        native_handle,
        target_mode != WindowMode::Desktop,
    );
    #[cfg(target_os = "windows")]
    desktop_mode::refresh_window_surface(native_handle);
    #[cfg(target_os = "windows")]
    if let Some(scale) = desktop_display_scale {
        let _ = app.emit("desktop-display-scale-changed", scale);
    } else if target_mode != WindowMode::Desktop {
        let _ = app.emit("desktop-display-scale-changed", 1.0f64);
    }
    Ok(target_mode)
}

#[tauri::command]
fn set_window_mode(app: AppHandle, mode: String) -> Result<String, String> {
    let mode = WindowMode::from_id(&mode).ok_or_else(|| "不支持的窗口模式".to_string())?;
    let current_mode = apply_window_mode(&app, mode)?;
    publish_window_mode(&app, current_mode);
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        persist_webview_window_state(&window);
    }
    Ok(current_mode.id().to_string())
}

#[tauri::command]
fn refresh_window_surface(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or_else(|| "主窗口不可用".to_string())?;
    #[cfg(target_os = "windows")]
    {
        let handle = window.hwnd().map_err(|error| error.to_string())?.0;
        desktop_mode::refresh_window_surface(desktop_mode::top_level_window(handle));
    }
    Ok(())
}

#[tauri::command]
fn desktop_display_scale(app: AppHandle) -> Result<f64, String> {
    #[cfg(target_os = "windows")]
    {
        let window = app
            .get_webview_window(MAIN_WINDOW)
            .ok_or_else(|| "主窗口不可用".to_string())?;
        let handle = window.hwnd().map_err(|error| error.to_string())?.0;
        if let Some(attachment) = read_desktop_attachment(&app) {
            return Ok(desktop_mode::adapt_to_monitor(
                desktop_mode::top_level_window(handle),
                attachment,
            ));
        }
    }
    Ok(1.0)
}

#[tauri::command]
fn hide_to_tray(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or_else(|| "主窗口不可用".to_string())?;
    #[cfg(target_os = "windows")]
    if let Ok(handle) = window.hwnd() {
        auto_hide::cancel_and_restore(&app, handle.0);
    }
    #[cfg(target_os = "windows")]
    suspend_desktop_binding(&app, &window)?;
    window.hide().map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn start_window_drag(window: tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SendMessageW, HTCAPTION, WM_NCLBUTTONDOWN,
        };

        let native_handle = window.hwnd().map_err(|error| error.to_string())?.0;
        // Delegate to Windows' non-client caption handling. This does not rely on
        // WebView pointer capture, so it works equally for every custom title bar.
        unsafe {
            ReleaseCapture();
            SendMessageW(native_handle, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0);
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    window.start_dragging().map_err(|error| error.to_string())
}

#[tauri::command]
fn start_window_resize(window: tauri::WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SendMessageW, HTBOTTOMRIGHT, WM_NCLBUTTONDOWN,
        };

        let native_handle = window.hwnd().map_err(|error| error.to_string())?.0;
        if let Ok(mut data) = window.app_handle().state::<WindowModeState>().data.lock() {
            data.user_resizing = true;
        }
        unsafe {
            ReleaseCapture();
            SendMessageW(native_handle, WM_NCLBUTTONDOWN, HTBOTTOMRIGHT as usize, 0);
        }
        if let Ok(mut data) = window.app_handle().state::<WindowModeState>().data.lock() {
            data.user_resizing = false;
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    window
        .start_resize_dragging(tauri::ResizeDirection::SouthEast)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn window_mode(app: AppHandle) -> Result<String, String> {
    Ok(read_window_mode(&app)?.id().to_string())
}

#[tauri::command]
fn load_bootstrap_data(store: tauri::State<'_, DataStore>) -> Result<data::BootstrapDto, String> {
    store.bootstrap().map_err(|error| error.to_string())
}

#[tauri::command]
fn mcp_status(
    state: tauri::State<'_, mcp_service::McpServiceState>,
) -> mcp_service::McpServiceSnapshot {
    state.verified_snapshot()
}

fn mcp_connection_listener(app: &AppHandle) -> mcp_service::McpConnectionListener {
    let event_app = app.clone();
    std::sync::Arc::new(move |connected| {
        let _ = event_app.emit("mylist-mcp-ai-connection-changed", connected);
    })
}

#[tauri::command]
fn set_mcp_enabled(
    app: AppHandle,
    store: tauri::State<'_, DataStore>,
    state: tauri::State<'_, mcp_service::McpServiceState>,
    enabled: bool,
) -> Result<mcp_service::McpServiceSnapshot, String> {
    let previous = store.mcp_enabled().map_err(|error| error.to_string())?;
    let snapshot = if enabled {
        let bridge_app = app.clone();
        let handler = std::sync::Arc::new(move |line: &str| {
            mcp_bridge::handle_request(Some(&bridge_app), line)
        });
        state.start_with_handler(Some(handler), Some(mcp_connection_listener(&app)))?;
        state.verified_snapshot()
    } else {
        state.stop()
    };
    if enabled && snapshot.status != mcp_service::McpServiceStatus::Online {
        return Err("mcp_start_failed".into());
    }
    if let Err(error) = store.save_mcp_enabled(enabled) {
        if previous {
            let bridge_app = app.clone();
            let handler = std::sync::Arc::new(move |line: &str| {
                mcp_bridge::handle_request(Some(&bridge_app), line)
            });
            let _ = state.start_with_handler(Some(handler), Some(mcp_connection_listener(&app)));
        } else {
            state.stop();
        }
        return Err(error.to_string());
    }
    Ok(snapshot)
}

#[tauri::command]
fn load_external_locale(
    app: AppHandle,
    locale: String,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    native_i18n::read_external_messages(&app, &locale)
}

#[tauri::command]
fn save_theme_setting(store: tauri::State<'_, DataStore>, theme: String) -> Result<String, String> {
    store.save_theme(&theme).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_interface_transparency_setting(
    store: tauri::State<'_, DataStore>,
    transparency: u8,
) -> Result<u8, String> {
    store
        .save_interface_transparency(transparency)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_locale_setting(
    app: AppHandle,
    store: tauri::State<'_, DataStore>,
    locale: String,
) -> Result<String, String> {
    let saved = store
        .save_locale(&locale)
        .map_err(|error| error.to_string())?;
    let mode = read_window_mode(&app).unwrap_or(WindowMode::Normal);
    synchronize_tray_checks(&app, mode);
    Ok(saved)
}

#[tauri::command]
fn sync_native_labels(app: AppHandle, labels: NativeLabels) -> Result<(), String> {
    let state = app.state::<NativeLabelsState>();
    let mut current = state
        .labels
        .lock()
        .map_err(|_| "native_labels_unavailable".to_string())?;
    *current = labels;
    drop(current);
    synchronize_tray_checks(&app, read_window_mode(&app).unwrap_or(WindowMode::Normal));
    Ok(())
}

#[tauri::command]
fn set_startup_enabled(
    app: AppHandle,
    store: tauri::State<'_, DataStore>,
    enabled: bool,
) -> Result<bool, String> {
    let previous = store.startup_enabled().map_err(|error| error.to_string())?;
    let autostart = app.autolaunch();
    let result = if enabled {
        autostart.enable()
    } else {
        autostart.disable()
    };
    if result.is_err() {
        return Err("开机启动设置失败".to_string());
    }
    if let Err(error) = store.save_startup_enabled(enabled) {
        let _ = if previous {
            autostart.enable()
        } else {
            autostart.disable()
        };
        return Err(error.to_string());
    }
    Ok(enabled)
}

#[tauri::command]
fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard()
        .write_text(text)
        .map_err(|_| "复制失败".to_string())
}

#[tauri::command]
fn read_clipboard_text(app: AppHandle) -> Result<String, String> {
    app.clipboard()
        .read_text()
        .map_err(|_| "读取剪贴板失败".to_string())
}

#[tauri::command]
fn mcp_install_prompt(app: AppHandle, locale: String) -> Result<String, String> {
    const FILE_NAME: &str = "Install MCP and Skill.en.md";
    let resource = app
        .path()
        .resource_dir()
        .map_err(|_| "安装引导文档不可用".to_string())?
        .join("docs")
        .join(FILE_NAME);
    let guide = if resource.exists() {
        resource
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("docs")
            .join(FILE_NAME)
    };
    if !guide.exists() {
        return Err("安装引导文档不可用".to_string());
    }
    let guide = guide.to_string_lossy();
    let guide = guide.strip_prefix(r"\\?\").unwrap_or(&guide);
    let instruction = match locale.as_str() {
        "zh-TW" => "按此文檔設定 MyLIST MCP 和 Skill：",
        "de" => "MyLIST MCP und Skill nach dieser Anleitung einrichten:",
        "fr" => "Configurez MyLIST MCP et le Skill avec ce guide :",
        "it" => "Configura MyLIST MCP e Skill con questa guida:",
        "es" => "Configura MyLIST MCP y la Skill con esta guía:",
        "ja" => "この手順で MyLIST MCP と Skill を設定：",
        "en" => "Configure MyLIST MCP and Skill using this guide:",
        _ => "按此文档配置 MyLIST MCP 和 Skill：",
    };
    Ok(format!("{instruction}\n{guide}\n"))
}

#[tauri::command]
fn export_plaintext_snapshot(
    app: AppHandle,
    store: tauri::State<'_, DataStore>,
) -> Result<Option<String>, String> {
    let Some(path) = app
        .dialog()
        .file()
        .add_filter(native_labels(&app).plaintext_file, &["json"])
        .set_file_name(format!("MyLIST_data_{}.dtodo.json", local_export_date()))
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|_| "导出路径不可用".to_string())?;
    let unique_path = unique_export_path(&path);
    store
        .write_plaintext_export(&unique_path)
        .map_err(|error| error.to_string())?;
    Ok(Some(unique_path.to_string_lossy().into_owned()))
}

#[tauri::command]
fn export_encrypted_snapshot(
    app: AppHandle,
    store: tauri::State<'_, DataStore>,
    password: String,
) -> Result<Option<String>, String> {
    let Some(path) = app
        .dialog()
        .file()
        .add_filter(native_labels(&app).encrypted_file, &["dtodo"])
        .set_file_name(format!("MyLIST_data_{}.dtodo", local_export_date()))
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|_| "导出路径不可用".to_string())?;
    let unique_path = unique_export_path(&path);
    store
        .write_encrypted_export(&unique_path, &password)
        .map_err(|error| error.to_string())?;
    Ok(Some(unique_path.to_string_lossy().into_owned()))
}

/// Executes an export that was requested by an MCP client. The save dialog and
/// optional password stay in the desktop process; the MCP client only sees the
/// final file name through `mylist_get_operation`.
#[tauri::command]
fn mcp_export_snapshot(
    app: AppHandle,
    store: tauri::State<'_, DataStore>,
    transfers: tauri::State<'_, mcp_transfer::McpTransferState>,
    operation_id: String,
    password: Option<String>,
) -> Result<(), String> {
    let operation = transfers
        .get(&operation_id)
        .map_err(|error| mcp::error(error, false).code)?;
    let encrypted = operation.operation == "export_encrypted";
    if (!encrypted && password.is_some()) || (encrypted && password.is_none()) {
        return Err(mcp::error(mcp::McpErrorCode::OperationRejected, false).code);
    }
    transfers
        .assert_operation(&operation_id, &operation.operation)
        .map_err(|error| mcp::error(error, false).code)?;
    let Some(path) = app
        .dialog()
        .file()
        .add_filter(
            if encrypted {
                native_labels(&app).encrypted_file
            } else {
                native_labels(&app).plaintext_file
            },
            if encrypted { &["dtodo"] } else { &["json"] },
        )
        .set_file_name(if encrypted {
            format!("MyLIST_data_{}.dtodo", local_export_date())
        } else {
            format!("MyLIST_data_{}.dtodo.json", local_export_date())
        })
        .blocking_save_file()
    else {
        transfers
            .cancel(&operation_id)
            .map_err(|error| mcp::error(error, false).code)?;
        return Ok(());
    };
    let path = path.into_path().map_err(|_| "导出路径不可用".to_string())?;
    let unique_path = unique_export_path(&path);
    let write = match password.as_deref() {
        Some(password) => store.write_encrypted_export(&unique_path, password),
        None => store.write_plaintext_export(&unique_path),
    };
    if let Err(error) = write {
        let _ = transfers.fail(&operation_id, "EXPORT_FAILED");
        return Err(error.to_string());
    }
    let file_name = unique_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("MyLIST_data");
    transfers
        .complete(&operation_id, serde_json::json!({"fileName": file_name}))
        .map_err(|error| mcp::error(error, false).code)
}

fn unique_export_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("MyLIST_data.dtodo.json");
    let (stem, suffix) = file_name
        .strip_suffix(".dtodo.json")
        .map(|stem| (stem, ".dtodo.json"))
        .unwrap_or_else(|| {
            (
                path.file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("MyLIST_data"),
                ".json",
            )
        });
    for number in 1..10_000 {
        let candidate = parent.join(format!("{stem} ({number}){suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}

fn local_export_date() -> String {
    // MyLIST is Windows-first and currently targets the local Chinese desktop
    // workflow. Convert UTC to China Standard Time without adding a new crate.
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        + 8 * 3_600;
    let days = seconds.div_euclid(86_400);
    let (year, month, day) = civil_date_from_unix_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_date_from_unix_days(days_since_epoch: i64) -> (i64, u32, u32) {
    // Howard Hinnant's civil-date conversion; day 0 is 1970-01-01.
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (mp + if mp < 10 { 3 } else { -9 }) as u32;
    (year + if month <= 2 { 1 } else { 0 }, month, day)
}

#[tauri::command]
fn preview_plaintext_import(
    app: AppHandle,
    store: tauri::State<'_, DataStore>,
    operation: String,
) -> Result<Option<ImportSelectionDto>, String> {
    preview_plaintext_import_with_session(app, store, operation, None)
}

fn preview_plaintext_import_with_session(
    app: AppHandle,
    store: tauri::State<'_, DataStore>,
    operation: String,
    session_id: Option<String>,
) -> Result<Option<ImportSelectionDto>, String> {
    if !matches!(operation.as_str(), "merge" | "replace") {
        return Err("不支持的导入方式".to_string());
    }
    let Some(path) = app
        .dialog()
        .file()
        .add_filter("MyLIST 数据", &["json", "dtodo"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|_| "导入路径不可用".to_string())?;
    let source_file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("导入数据")
        .to_string();
    let encoded = fs::read(&path).map_err(|error| format!("无法读取导入文件：{error}"))?;
    let session_id = session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let state = app.state::<PendingImportState>();
    let mut pending = state
        .pending
        .lock()
        .map_err(|_| "导入状态不可用".to_string())?;
    if crypto::is_encrypted_export(&encoded) {
        *pending = Some(PendingImport::Encrypted {
            id: session_id.clone(),
            encoded,
            source_file_name: source_file_name.clone(),
            operation: operation.clone(),
        });
        return Ok(Some(ImportSelectionDto {
            kind: "password".into(),
            session_id,
            source_file_name,
            operation,
            preview: None,
        }));
    }
    let (package, source_file_name) = store
        .read_plaintext_import_bytes(&encoded, source_file_name)
        .map_err(|error| error.to_string())?;
    let mut preview = store
        .preview_import_package(&package, &source_file_name)
        .map_err(|error| error.to_string())?;
    *pending = Some(PendingImport::Ready {
        id: session_id.clone(),
        package,
        source_file_name: source_file_name.clone(),
        operation: operation.clone(),
    });
    preview.session_id = session_id;
    Ok(Some(ImportSelectionDto {
        kind: "preview".into(),
        session_id: preview.session_id.clone(),
        source_file_name,
        operation,
        preview: Some(preview),
    }))
}

#[tauri::command]
fn mcp_preview_import(
    app: AppHandle,
    store: tauri::State<'_, DataStore>,
    transfers: tauri::State<'_, mcp_transfer::McpTransferState>,
    operation_id: String,
) -> Result<Option<ImportSelectionDto>, String> {
    let operation = transfers
        .get(&operation_id)
        .map_err(|error| mcp::error(error, false).code)?;
    let import_mode = match operation.operation.as_str() {
        "import_merge" => "merge",
        "import_replace" => "replace",
        _ => return Err(mcp::error(mcp::McpErrorCode::OperationRejected, false).code),
    };
    transfers
        .assert_operation(&operation_id, &operation.operation)
        .map_err(|error| mcp::error(error, false).code)?;
    let selection = preview_plaintext_import_with_session(
        app,
        store,
        import_mode.to_string(),
        Some(operation_id.clone()),
    )?;
    if let Some(selection) = selection.as_ref() {
        if let Some(preview) = selection.preview.as_ref() {
            transfers
                .set_preview(
                    &operation_id,
                    serde_json::to_value(preview).map_err(|_| "导入预检不可用")?,
                )
                .map_err(|error| mcp::error(error, false).code)?;
        }
    } else {
        transfers
            .cancel(&operation_id)
            .map_err(|error| mcp::error(error, false).code)?;
    }
    Ok(selection)
}

#[tauri::command]
fn preview_pending_encrypted_import(
    app: AppHandle,
    store: tauri::State<'_, DataStore>,
    session_id: String,
    password: String,
) -> Result<data::ImportPreviewDto, String> {
    let state = app.state::<PendingImportState>();
    let pending = state
        .pending
        .lock()
        .map_err(|_| "导入状态不可用".to_string())?;
    let (encoded, source_file_name, operation) = match pending.as_ref() {
        Some(PendingImport::Encrypted {
            id,
            encoded,
            source_file_name,
            operation,
        }) if id == &session_id => (encoded.clone(), source_file_name.clone(), operation.clone()),
        _ => return Err("导入预检已失效，请重新选择文件".to_string()),
    };
    drop(pending);
    let decrypted =
        crypto::decrypt_export(&encoded, &password).map_err(|error| error.to_string())?;
    let (package, source_file_name) = store
        .read_plaintext_import_bytes(&decrypted, source_file_name)
        .map_err(|error| error.to_string())?;
    let mut preview = store
        .preview_import_package(&package, &source_file_name)
        .map_err(|error| error.to_string())?;
    preview.session_id = session_id.clone();
    let mut pending = state
        .pending
        .lock()
        .map_err(|_| "导入状态不可用".to_string())?;
    *pending = Some(PendingImport::Ready {
        id: session_id,
        package,
        source_file_name,
        operation,
    });
    Ok(preview)
}

#[tauri::command]
fn mcp_preview_pending_encrypted_import(
    app: AppHandle,
    store: tauri::State<'_, DataStore>,
    transfers: tauri::State<'_, mcp_transfer::McpTransferState>,
    operation_id: String,
    password: String,
) -> Result<data::ImportPreviewDto, String> {
    let preview = preview_pending_encrypted_import(app, store, operation_id.clone(), password)?;
    transfers
        .set_preview(
            &operation_id,
            serde_json::to_value(&preview).map_err(|_| "导入预检不可用")?,
        )
        .map_err(|error| mcp::error(error, false).code)?;
    Ok(preview)
}

#[tauri::command]
fn apply_pending_plaintext_import(
    app: AppHandle,
    store: tauri::State<'_, DataStore>,
    session_id: String,
    operation: String,
) -> Result<data::ImportResultDto, String> {
    let state = app.state::<PendingImportState>();
    let pending = state
        .pending
        .lock()
        .map_err(|_| "导入状态不可用".to_string())?;
    let result = match pending.as_ref() {
        Some(PendingImport::Ready {
            id,
            package,
            source_file_name,
            operation: pending_operation,
        }) if id == &session_id && pending_operation == &operation => match operation.as_str() {
            "merge" => store
                .import_plaintext_package(package, source_file_name)
                .map_err(|error| error.to_string())?,
            "replace" => store
                .replace_plaintext_package(package, source_file_name)
                .map_err(|error| error.to_string())?,
            _ => return Err("不支持的导入方式".to_string()),
        },
        Some(PendingImport::Encrypted { .. }) => return Err("请先输入导入密码".to_string()),
        _ => return Err("导入预检已失效，请重新选择文件".to_string()),
    };
    drop(pending);
    let mut pending = state
        .pending
        .lock()
        .map_err(|_| "导入状态不可用".to_string())?;
    *pending = None;
    Ok(result)
}

#[tauri::command]
fn mcp_apply_import(
    app: AppHandle,
    store: tauri::State<'_, DataStore>,
    transfers: tauri::State<'_, mcp_transfer::McpTransferState>,
    operation_id: String,
) -> Result<data::ImportResultDto, String> {
    let transfer = transfers
        .get(&operation_id)
        .map_err(|error| mcp::error(error, false).code)?;
    let operation = match transfer.operation.as_str() {
        "import_merge" => "merge",
        "import_replace" => "replace",
        _ => return Err(mcp::error(mcp::McpErrorCode::OperationRejected, false).code),
    };
    if transfer.status != "awaiting_confirmation" {
        return Err(mcp::error(mcp::McpErrorCode::ConfirmationRequired, false).code);
    }
    let result =
        apply_pending_plaintext_import(app, store, operation_id.clone(), operation.to_string())?;
    transfers
        .complete(
            &operation_id,
            serde_json::to_value(&result).map_err(|_| "导入结果不可用")?,
        )
        .map_err(|error| mcp::error(error, false).code)?;
    Ok(result)
}

#[tauri::command]
fn cancel_mcp_transfer(
    transfers: tauri::State<'_, mcp_transfer::McpTransferState>,
    operation_id: String,
) -> Result<(), String> {
    transfers
        .cancel(&operation_id)
        .map_err(|error| mcp::error(error, false).code)
}

#[tauri::command]
fn create_category(
    store: tauri::State<'_, DataStore>,
    input: data::CreateCategoryInput,
) -> Result<data::CategoryDto, String> {
    store
        .create_category(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn update_category(
    store: tauri::State<'_, DataStore>,
    input: data::UpdateCategoryInput,
) -> Result<data::CategoryDto, String> {
    store
        .update_category(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_category(
    store: tauri::State<'_, DataStore>,
    id: String,
    target_category_id: Option<String>,
) -> Result<(), String> {
    store
        .delete_category(&id, target_category_id.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn restore_default_categories(store: tauri::State<'_, DataStore>) -> Result<(), String> {
    store
        .restore_default_categories()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_tasks(
    store: tauri::State<'_, DataStore>,
    status: String,
) -> Result<Vec<data::TaskDto>, String> {
    store.list_tasks(&status).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_task(store: tauri::State<'_, DataStore>, id: String) -> Result<data::TaskDto, String> {
    store.get_task(&id).map_err(|error| error.to_string())
}

#[tauri::command]
fn create_task(
    store: tauri::State<'_, DataStore>,
    input: data::CreateTaskInput,
) -> Result<data::TaskDto, String> {
    store.create_task(input).map_err(|error| error.to_string())
}

#[tauri::command]
fn update_task(
    store: tauri::State<'_, DataStore>,
    input: data::UpdateTaskInput,
) -> Result<data::TaskDto, String> {
    store.update_task(input).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_task_recurrence(
    store: tauri::State<'_, DataStore>,
    id: String,
    recurrence: Option<data::RecurrenceConfig>,
) -> Result<data::TaskDto, String> {
    store
        .save_task_recurrence(&id, recurrence)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn settle_due_recurrences(store: tauri::State<'_, DataStore>) -> Result<usize, String> {
    store
        .settle_due_recurrences()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_task_status(
    store: tauri::State<'_, DataStore>,
    id: String,
    status: String,
) -> Result<data::TaskDto, String> {
    store
        .set_task_status(&id, &status)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_task(store: tauri::State<'_, DataStore>, id: String) -> Result<(), String> {
    store.delete_task(&id).map_err(|error| error.to_string())
}

fn configure_tray(app: &tauri::App) -> tauri::Result<()> {
    let labels = native_labels(app.handle());
    let open_main_item = MenuItem::with_id(app, OPEN_MAIN, labels.open_main, true, None::<&str>)?;
    let topmost_item =
        CheckMenuItem::with_id(app, TOPMOST_MODE, labels.topmost, true, false, None::<&str>)?;
    let normal_item =
        CheckMenuItem::with_id(app, NORMAL_MODE, labels.normal, true, true, None::<&str>)?;
    let desktop_item =
        CheckMenuItem::with_id(app, DESKTOP_MODE, labels.desktop, true, false, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, QUIT, labels.quit, true, None::<&str>)?;
    let mode_separator = PredefinedMenuItem::separator(app)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &open_main_item,
            &mode_separator,
            &topmost_item,
            &normal_item,
            &desktop_item,
            &separator,
            &quit_item,
        ],
    )?;

    let controls_state = app.state::<TrayModeControlsState>();
    if let Ok(mut controls) = controls_state.controls.lock() {
        *controls = Some(TrayModeControls {
            open_main: open_main_item,
            topmost: topmost_item,
            normal: normal_item,
            desktop: desktop_item,
            quit: quit_item,
        });
    }

    TrayIconBuilder::with_id("mylist-tray")
        .icon(
            app.default_window_icon()
                .expect("missing application icon")
                .clone(),
        )
        .tooltip("MyLIST")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| {
            if let Some(target_mode) = WindowMode::from_id(event.id.as_ref()) {
                match apply_window_mode(app, target_mode) {
                    Ok(mode) => {
                        publish_window_mode(app, mode);
                        // A desktop-bound window must remain behind ordinary windows.
                        // Bringing it to the foreground here would undermine that mode.
                        if mode == WindowMode::Desktop {
                            let _ = restore_for_current_mode(app);
                        } else {
                            let _ = restore_and_focus(app);
                        }
                    }
                    Err(_) => {
                        if let Ok(mode) = read_window_mode(app) {
                            synchronize_tray_checks(app, mode);
                        }
                    }
                }
            } else if event.id.as_ref() == OPEN_MAIN {
                let _ = open_main_from_tray(app);
            } else if event.id.as_ref() == QUIT {
                #[cfg(target_os = "windows")]
                if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                    let _ = window.hide();
                    let _ = suspend_desktop_binding(app, &window);
                }
                app.exit(0);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                let _ = restore_for_current_mode(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .manage(WindowModeState::default())
        .manage(mcp_service::McpServiceState::default())
        .manage(TrayModeControlsState::default())
        .manage(NativeLabelsState::default())
        .manage(PendingImportState::default())
        .manage(mcp_confirmation::McpConfirmationState::default())
        .manage(mcp_transfer::McpTransferState::default())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("MyLIST")
                .build(),
        )
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init());
    #[cfg(target_os = "windows")]
    let builder = builder.manage(auto_hide::AutoHideState::default());
    let app = builder
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = restore_for_current_mode(app);
        }))
        .setup(|app| {
            let store = DataStore::open(app.handle()).map_err(|error| {
                #[cfg(target_os = "windows")]
                show_startup_data_error(&error.to_string());
                error.to_string()
            })?;
            let saved_window_state = store.window_state().ok().flatten();
            if store.startup_enabled().unwrap_or(true) {
                let _ = app.autolaunch().enable();
            }
            app.manage(store);
            if app.state::<DataStore>().mcp_enabled().unwrap_or(false) {
                let bridge_app = app.handle().clone();
                let handler = std::sync::Arc::new(move |line: &str| {
                    mcp_bridge::handle_request(Some(&bridge_app), line)
                });
                let _ = app
                    .state::<mcp_service::McpServiceState>()
                    .start_with_handler(Some(handler), Some(mcp_connection_listener(app.handle())));
            }
            native_i18n::ensure_external_locale_files(app.handle())?;
            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                if let Some(saved) = saved_window_state.as_ref() {
                    restore_window_geometry(&window, saved);
                } else {
                    ensure_minimum_window_size(&window);
                }
                let native_handle = desktop_mode::top_level_window(
                    window.hwnd().map_err(|error| error.to_string())?.0,
                );
                let _ = window.set_maximizable(false);
                let _ = window_shape::disable_maximization(native_handle);
                let _ = window_shape::apply_rounded_region(native_handle);
                auto_hide::start_cursor_monitor(app.handle().clone(), native_handle);
                if let Some(saved) = saved_window_state.as_ref() {
                    if let Some(mode) = WindowMode::from_id(&saved.mode) {
                        let _ = apply_window_mode(app.handle(), mode);
                        publish_window_mode(app.handle(), mode);
                    }
                }
            }
            configure_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                #[cfg(target_os = "windows")]
                if let Ok(handle) = window.hwnd() {
                    auto_hide::cancel_and_restore(window.app_handle(), handle.0);
                }
                #[cfg(target_os = "windows")]
                if let Some(webview) = window.app_handle().get_webview_window(MAIN_WINDOW) {
                    let _ = suspend_desktop_binding(window.app_handle(), &webview);
                }
                persist_window_state(window);
                let _ = window.hide();
            }
            WindowEvent::Moved(_) =>
            {
                #[cfg(target_os = "windows")]
                if let Ok(handle) = window.hwnd() {
                    let mode = read_window_mode(window.app_handle()).unwrap_or(WindowMode::Normal);
                    if mode == WindowMode::Desktop {
                        let native_handle = desktop_mode::top_level_window(handle.0);
                        if let Some(attachment) = read_desktop_attachment(window.app_handle()) {
                            let scale = desktop_mode::adapt_to_monitor(native_handle, attachment);
                            let _ = window
                                .app_handle()
                                .emit("desktop-display-scale-changed", scale);
                        }
                        desktop_mode::refresh_window_surface(native_handle);
                    } else {
                        normalize_tracked_window_size(window, handle.0);
                    }
                    persist_window_state(window);
                    auto_hide::on_window_moved(
                        window.app_handle().clone(),
                        handle.0,
                        mode != WindowMode::Desktop,
                    );
                }
            }
            WindowEvent::Focused(_) =>
            {
                #[cfg(target_os = "windows")]
                if let Ok(handle) = window.hwnd() {
                    let native_handle = desktop_mode::top_level_window(handle.0);
                    if read_window_mode(window.app_handle()).unwrap_or(WindowMode::Normal)
                        != WindowMode::Desktop
                    {
                        let _ = window_shape::disable_maximization(native_handle);
                        let _ = window_shape::apply_rounded_region(native_handle);
                    }
                    desktop_mode::refresh_window_surface(native_handle);
                }
            }
            WindowEvent::Resized(_) => {
                #[cfg(target_os = "windows")]
                if let Ok(window_handle) = window.hwnd() {
                    let native_handle = desktop_mode::top_level_window(window_handle.0);
                    let _ = window_shape::apply_rounded_region(native_handle);
                    update_tracked_window_size(window, native_handle);
                }
                persist_window_state(window);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            set_window_mode,
            refresh_window_surface,
            desktop_display_scale,
            hide_to_tray,
            start_window_drag,
            start_window_resize,
            window_mode,
            load_bootstrap_data,
            mcp_status,
            set_mcp_enabled,
            load_external_locale,
            save_theme_setting,
            save_interface_transparency_setting,
            save_locale_setting,
            sync_native_labels,
            set_startup_enabled,
            copy_text,
            read_clipboard_text,
            mcp_install_prompt,
            export_plaintext_snapshot,
            export_encrypted_snapshot,
            mcp_export_snapshot,
            preview_plaintext_import,
            mcp_preview_import,
            preview_pending_encrypted_import,
            mcp_preview_pending_encrypted_import,
            apply_pending_plaintext_import,
            mcp_apply_import,
            cancel_mcp_transfer,
            create_category,
            update_category,
            delete_category,
            restore_default_categories,
            list_tasks,
            get_task,
            create_task,
            update_task,
            save_task_recurrence,
            settle_due_recurrences,
            set_task_status,
            delete_task,
            approve_mcp_confirmation,
            reject_mcp_confirmation
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    app.run(|app_handle, event| {
        if matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) {
            app_handle
                .state::<mcp_service::McpServiceState>()
                .shutdown();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{WindowMode, DESKTOP_MODE, NORMAL_MODE, TOPMOST_MODE};

    #[test]
    fn every_supported_window_mode_has_a_stable_ipc_id() {
        assert_eq!(WindowMode::Topmost.id(), TOPMOST_MODE);
        assert_eq!(WindowMode::Normal.id(), NORMAL_MODE);
        assert_eq!(WindowMode::Desktop.id(), DESKTOP_MODE);
        assert_eq!(WindowMode::from_id("unknown"), None);
    }
}
