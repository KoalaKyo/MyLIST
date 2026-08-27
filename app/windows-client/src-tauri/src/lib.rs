#[cfg(target_os = "windows")]
mod auto_hide;
mod crypto;
mod data;
#[cfg(target_os = "windows")]
mod desktop_mode;
mod mcp;
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

const MAIN_WINDOW: &str = "main";
const TOPMOST_MODE: &str = "mode-topmost";
const NORMAL_MODE: &str = "mode-normal";
const DESKTOP_MODE: &str = "mode-desktop";
const OPEN_MAIN: &str = "open-main";
const QUIT: &str = "quit";

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
            }),
        }
    }
}

struct TrayModeControls {
    open_main: MenuItem<tauri::Wry>,
    topmost: CheckMenuItem<tauri::Wry>,
    normal: CheckMenuItem<tauri::Wry>,
    desktop: CheckMenuItem<tauri::Wry>,
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

fn native_labels(app: &AppHandle) -> NativeLabels {
    app.state::<NativeLabelsState>()
        .labels
        .lock()
        .map(|labels| labels.clone())
        .unwrap_or_default()
}

fn restore_and_focus(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        #[cfg(target_os = "windows")]
        if let Ok(handle) = window.hwnd() {
            auto_hide::cancel_and_restore(app, handle.0);
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

    window.show()?;
    window.unminimize()?;

    if mode != WindowMode::Desktop {
        return window.set_focus();
    }

    #[cfg(target_os = "windows")]
    {
        let native_handle = desktop_mode::top_level_window(window.hwnd()?.0);
        let attachment = {
            let state = app.state::<WindowModeState>();
            state
                .data
                .lock()
                .ok()
                .and_then(|data| data.desktop_attachment)
        };
        if let Some(attachment) = attachment {
            let _ = desktop_mode::reapply(native_handle, attachment);
        }
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
    let mode = read_window_mode(app)
        .unwrap_or(WindowMode::Normal)
        .id()
        .to_string();
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
    let mut data = state
        .data
        .lock()
        .map_err(|_| "窗口模式状态不可用".to_string())?;

    if data.mode == target_mode {
        return Ok(target_mode);
    }

    #[cfg(target_os = "windows")]
    if let Some(attachment) = data.desktop_attachment.take() {
        desktop_mode::detach(native_handle, attachment);
    }

    match target_mode {
        WindowMode::Topmost => {
            window
                .set_always_on_top(true)
                .map_err(|error| error.to_string())?;
        }
        WindowMode::Normal => {
            if data.mode == WindowMode::Topmost {
                window
                    .set_always_on_top(false)
                    .map_err(|error| error.to_string())?;
            }
        }
        WindowMode::Desktop => {
            // Calling Tauri's no-op "unset topmost" on an already normal window
            // can asynchronously restore its parent to the desktop root. Only do
            // it when a real topmost state must be cleared.
            if data.mode == WindowMode::Topmost {
                window
                    .set_always_on_top(false)
                    .map_err(|error| error.to_string())?;
            }
            #[cfg(target_os = "windows")]
            {
                match desktop_mode::attach(native_handle) {
                    Ok(attachment) => data.desktop_attachment = Some(attachment),
                    Err(error) => {
                        // A failed native reparent must never strand the test
                        // window beneath the desktop icons.
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

    data.mode = target_mode;
    drop(data);
    #[cfg(target_os = "windows")]
    auto_hide::recheck_after_mode_change(
        app.clone(),
        native_handle,
        target_mode != WindowMode::Desktop,
    );
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
fn hide_to_tray(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(MAIN_WINDOW)
        .ok_or_else(|| "主窗口不可用".to_string())?;
    #[cfg(target_os = "windows")]
    if let Ok(handle) = window.hwnd() {
        auto_hide::cancel_and_restore(&app, handle.0);
    }
    window.hide().map_err(|error| error.to_string())
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
        unsafe {
            ReleaseCapture();
            SendMessageW(native_handle, WM_NCLBUTTONDOWN, HTBOTTOMRIGHT as usize, 0);
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
    let session_id = uuid::Uuid::new_v4().to_string();
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
        .manage(TrayModeControlsState::default())
        .manage(NativeLabelsState::default())
        .manage(PendingImportState::default())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("MyLIST")
                .build(),
        )
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init());
    #[cfg(target_os = "windows")]
    let builder = builder.manage(auto_hide::AutoHideState::default());
    builder
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
            native_i18n::ensure_external_locale_files(app.handle())?;
            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                if let Some(saved) = saved_window_state.as_ref() {
                    let _ = window.set_size(tauri::PhysicalSize::new(
                        saved.width.max(350),
                        saved.height.max(530),
                    ));
                    let _ = window.set_position(tauri::PhysicalPosition::new(saved.x, saved.y));
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
                persist_window_state(window);
                let _ = window.hide();
            }
            WindowEvent::Moved(_) =>
            {
                #[cfg(target_os = "windows")]
                if let Ok(handle) = window.hwnd() {
                    persist_window_state(window);
                    let mode = read_window_mode(window.app_handle()).unwrap_or(WindowMode::Normal);
                    auto_hide::on_window_moved(
                        window.app_handle().clone(),
                        handle.0,
                        mode != WindowMode::Desktop,
                    );
                }
            }
            WindowEvent::Resized(_) => {
                #[cfg(target_os = "windows")]
                if let Ok(window_handle) = window.hwnd() {
                    let native_handle = desktop_mode::top_level_window(window_handle.0);
                    let _ = window_shape::apply_rounded_region(native_handle);
                }
                persist_window_state(window);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            set_window_mode,
            hide_to_tray,
            start_window_drag,
            start_window_resize,
            window_mode,
            load_bootstrap_data,
            load_external_locale,
            save_theme_setting,
            save_locale_setting,
            sync_native_labels,
            set_startup_enabled,
            copy_text,
            export_plaintext_snapshot,
            export_encrypted_snapshot,
            preview_plaintext_import,
            preview_pending_encrypted_import,
            apply_pending_plaintext_import,
            create_category,
            update_category,
            delete_category,
            restore_default_categories,
            list_tasks,
            get_task,
            create_task,
            update_task,
            set_task_status,
            delete_task
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
