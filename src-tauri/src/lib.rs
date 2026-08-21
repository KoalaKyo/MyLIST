mod data;
#[cfg(target_os = "windows")]
mod desktop_mode;
#[cfg(target_os = "windows")]
mod window_shape;

use std::sync::Mutex;

use data::DataStore;

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent,
};

const MAIN_WINDOW: &str = "main";
const TOPMOST_MODE: &str = "mode-topmost";
const NORMAL_MODE: &str = "mode-normal";
const DESKTOP_MODE: &str = "mode-desktop";
const QUIT: &str = "quit";

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
    topmost: CheckMenuItem<tauri::Wry>,
    normal: CheckMenuItem<tauri::Wry>,
    desktop: CheckMenuItem<tauri::Wry>,
}

#[derive(Default)]
struct TrayModeControlsState {
    controls: Mutex<Option<TrayModeControls>>,
}

fn restore_and_focus(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
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

fn read_window_mode(app: &AppHandle) -> Result<WindowMode, String> {
    let state = app.state::<WindowModeState>();
    let data = state
        .data
        .lock()
        .map_err(|_| "窗口模式状态不可用".to_string())?;
    Ok(data.mode)
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
    Ok(target_mode)
}

#[tauri::command]
fn set_window_mode(app: AppHandle, mode: String) -> Result<String, String> {
    let mode = WindowMode::from_id(&mode).ok_or_else(|| "不支持的窗口模式".to_string())?;
    let current_mode = apply_window_mode(&app, mode)?;
    publish_window_mode(&app, current_mode);
    Ok(current_mode.id().to_string())
}

#[tauri::command]
fn hide_to_tray(app: AppHandle) -> Result<(), String> {
    app.get_webview_window(MAIN_WINDOW)
        .ok_or_else(|| "主窗口不可用".to_string())?
        .hide()
        .map_err(|error| error.to_string())
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
        return Ok(());
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
        return Ok(());
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
fn save_theme_setting(store: tauri::State<'_, DataStore>, theme: String) -> Result<String, String> {
    store.save_theme(&theme).map_err(|error| error.to_string())
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
    let topmost_item =
        CheckMenuItem::with_id(app, TOPMOST_MODE, "置顶模式", true, false, None::<&str>)?;
    let normal_item =
        CheckMenuItem::with_id(app, NORMAL_MODE, "普通模式", true, true, None::<&str>)?;
    let desktop_item =
        CheckMenuItem::with_id(app, DESKTOP_MODE, "桌面模式", true, false, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, QUIT, "退出", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
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
    tauri::Builder::default()
        .manage(WindowModeState::default())
        .manage(TrayModeControlsState::default())
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = restore_for_current_mode(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let store = DataStore::open(app.handle()).map_err(|error| error.to_string())?;
            app.manage(store);
            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                let native_handle = desktop_mode::top_level_window(
                    window.hwnd().map_err(|error| error.to_string())?.0,
                );
                let _ = window_shape::apply_rounded_region(native_handle);
            }
            configure_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Resized(_) =>
            {
                #[cfg(target_os = "windows")]
                if let Ok(window_handle) = window.hwnd() {
                    let native_handle = desktop_mode::top_level_window(window_handle.0);
                    let _ = window_shape::apply_rounded_region(native_handle);
                }
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
            save_theme_setting,
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
