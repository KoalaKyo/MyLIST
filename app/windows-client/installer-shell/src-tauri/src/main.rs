#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    env, fs,
    ffi::{OsStr, OsString},
    os::windows::ffi::{OsStrExt, OsStringExt},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const PAYLOAD: &[u8] = include_bytes!(env!("MYLIST_INSTALLER_PAYLOAD"));
const BUILD_MODE: &str = env!("MYLIST_SHELL_MODE");

#[link(name = "kernel32")]
extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(key: isize, sub_key: *const u16, options: u32, access: u32, result: *mut isize) -> i32;
    fn RegQueryValueExW(key: isize, value_name: *const u16, reserved: *mut u32, value_type: *mut u32, data: *mut u8, data_size: *mut u32) -> i32;
    fn RegCloseKey(key: isize) -> i32;
}

#[tauri::command]
fn shell_mode() -> &'static str { BUILD_MODE }

#[tauri::command]
fn system_locale() -> &'static str {
    let language = unsafe { GetUserDefaultUILanguage() };
    match language {
        0x0404 | 0x0c04 | 0x1404 => "zh-TW",
        0x0804 | 0x1004 => "zh-CN",
        0x0411 => "ja",
        _ => match language & 0x03ff {
            0x0007 => "de",
            0x000c => "fr",
            0x0010 => "it",
            0x000a => "es",
            0x0009 => "en",
            0x0004 => "zh-CN",
            _ => "en",
        },
    }
}

fn wide(value: &str) -> Vec<u16> { OsStr::new(value).encode_wide().chain(Some(0)).collect() }

fn registry_value(root: isize, key: &str, value: Option<&str>, view: u32) -> Option<PathBuf> {
    const KEY_READ: u32 = 0x20019;
    const REG_SZ: u32 = 1;
    const ERROR_SUCCESS: i32 = 0;
    let mut handle = 0isize;
    let key = wide(key);
    if unsafe { RegOpenKeyExW(root, key.as_ptr(), 0, KEY_READ | view, &mut handle) } != ERROR_SUCCESS { return None; }
    let value = value.map(wide);
    let value_ptr = value.as_ref().map_or(std::ptr::null(), |name| name.as_ptr());
    let mut value_type = 0u32;
    let mut byte_count = 0u32;
    let size_result = unsafe { RegQueryValueExW(handle, value_ptr, std::ptr::null_mut(), &mut value_type, std::ptr::null_mut(), &mut byte_count) };
    if size_result != ERROR_SUCCESS || value_type != REG_SZ || byte_count < 2 {
        unsafe { RegCloseKey(handle); }
        return None;
    }
    let mut data = vec![0u16; byte_count as usize / 2];
    let query_result = unsafe { RegQueryValueExW(handle, value_ptr, std::ptr::null_mut(), &mut value_type, data.as_mut_ptr().cast(), &mut byte_count) };
    unsafe { RegCloseKey(handle); }
    if query_result != ERROR_SUCCESS { return None; }
    while data.last() == Some(&0) { data.pop(); }
    let value = OsString::from_wide(&data).to_string_lossy().trim().trim_matches('"').to_string();
    if value.is_empty() { None } else { Some(PathBuf::from(value)) }
}

fn installed_directory() -> Option<PathBuf> {
    const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\MyLIST";
    const PRODUCT_KEY: &str = r"Software\mylist\MyLIST";
    const HKEY_CURRENT_USER: isize = 0x80000001u32 as isize;
    const HKEY_LOCAL_MACHINE: isize = 0x80000002u32 as isize;
    const KEY_WOW64_64KEY: u32 = 0x0100;
    const KEY_WOW64_32KEY: u32 = 0x0200;
    let data_dir = env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(env::temp_dir).join("com.mylist.desktop");
    if let Ok(saved) = fs::read_to_string(data_dir.join(".install-location")) {
        let candidate = PathBuf::from(saved.trim());
        if candidate.is_absolute() && candidate.join("windows-client.exe").is_file() { return Some(candidate); }
    }
    for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
        for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            let candidates = [
                registry_value(root, UNINSTALL_KEY, Some("InstallLocation"), view),
                registry_value(root, PRODUCT_KEY, None, view),
            ];
            for candidate in candidates.into_iter().flatten() {
                if candidate.is_absolute() && candidate.join("windows-client.exe").is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    shortcut_install_directory()
}

fn shortcut_install_directory() -> Option<PathBuf> {
    const SCRIPT: &str = r#"[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); $shell=New-Object -ComObject WScript.Shell; $roots=@("$env:APPDATA\Microsoft\Windows\Start Menu\Programs","$env:USERPROFILE\Desktop","$env:PUBLIC\Desktop","$env:ProgramData\Microsoft\Windows\Start Menu\Programs"); Get-ChildItem -LiteralPath $roots -Filter 'MyLIST.lnk' -Recurse -ErrorAction SilentlyContinue | ForEach-Object { $target=$shell.CreateShortcut($_.FullName).TargetPath; if ((Split-Path -Leaf $target) -ieq 'windows-client.exe' -and (Test-Path -LiteralPath $target -PathType Leaf)) { Split-Path -Parent $target; break } }"#;
    let mut command = Command::new("powershell.exe");
    command.creation_flags(0x08000000).args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);
    let output = command.output().ok()?;
    if !output.status.success() { return None; }
    let candidate = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    if candidate.is_absolute() && candidate.join("windows-client.exe").is_file() { Some(candidate) } else { None }
}

#[tauri::command]
fn default_install_directory() -> String {
    installed_directory()
        .unwrap_or_else(|| env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(env::temp_dir).join("MyLIST"))
        .to_string_lossy()
        .into_owned()
}

fn folder_title(locale: &str) -> &'static str {
    match locale {
        "zh-CN" => "选择 MyLIST 安装目录", "zh-TW" => "選擇 MyLIST 安裝目錄",
        "de" => "MyLIST-Installationsort auswählen", "fr" => "Choisir le dossier d’installation de MyLIST",
        "it" => "Scegli il percorso di installazione di MyLIST", "es" => "Elegir ubicación de instalación de MyLIST",
        "ja" => "MyLIST のインストール先を選択", _ => "Choose MyLIST install location",
    }
}

#[tauri::command]
fn choose_install_directory(current: String, locale: String) -> Option<String> {
    let mut dialog = rfd::FileDialog::new().set_title(folder_title(&locale));
    if Path::new(&current).exists() { dialog = dialog.set_directory(current); }
    else if let Some(parent) = Path::new(&current).parent().filter(|path| path.exists()) { dialog = dialog.set_directory(parent); }
    dialog.pick_folder().map(|path| path.to_string_lossy().into_owned())
}

fn valid(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value.trim());
    if value.trim().is_empty() { Err("ERR_DIRECTORY_REQUIRED".into()) }
    else if !path.is_absolute() { Err("ERR_DIRECTORY_ABSOLUTE".into()) }
    else { Ok(path) }
}

fn persist_install_state(locale: &str, install_dir: &Path) -> Result<(), String> {
    if !matches!(locale, "zh-CN" | "zh-TW" | "en" | "de" | "fr" | "it" | "es" | "ja") {
        return Err("ERR_SAVE_LANGUAGE:unsupported locale".into());
    }
    let data_dir = env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(env::temp_dir).join("com.mylist.desktop");
    fs::create_dir_all(&data_dir).map_err(|error| format!("ERR_SAVE_LANGUAGE:{error}"))?;
    fs::write(data_dir.join(".installer-locale"), locale).map_err(|error| format!("ERR_SAVE_LANGUAGE:{error}"))?;
    let marker = data_dir.join(".install-location");
    let temporary = data_dir.join(".install-location.tmp");
    fs::write(&temporary, install_dir.to_string_lossy().as_bytes()).map_err(|error| format!("ERR_SAVE_LANGUAGE:{error}"))?;
    if marker.exists() { fs::remove_file(&marker).map_err(|error| format!("ERR_SAVE_LANGUAGE:{error}"))?; }
    fs::rename(temporary, marker).map_err(|error| format!("ERR_SAVE_LANGUAGE:{error}"))
}

fn remove_matching_install_location(install_dir: &Path) {
    let data_dir = env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(env::temp_dir).join("com.mylist.desktop");
    let marker = data_dir.join(".install-location");
    let matches = fs::read_to_string(&marker).ok()
        .map(|saved| PathBuf::from(saved.trim()).eq(install_dir))
        .unwrap_or(false);
    if matches { let _ = fs::remove_file(marker); }
    let _ = fs::remove_file(data_dir.join(".install-location.tmp"));
}

#[tauri::command]
async fn install_mylist(directory: String, locale: String) -> Result<(), String> {
    if BUILD_MODE != "installer" { return Err("ERR_INSTALL_NOT_SUPPORTED".into()); }
    let dir = valid(&directory)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_millis();
    let payload = env::temp_dir().join(format!("mylist-{stamp}.exe"));
    fs::write(&payload, PAYLOAD).map_err(|error| format!("ERR_PREPARE_INSTALLER:{error}"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut command = Command::new(&payload);
        command.arg("/S");
        command.raw_arg(format!("/D={}", dir.display()));
        let status = command.status().map_err(|error| format!("ERR_START_INSTALLER:{error}"));
        let _ = fs::remove_file(payload);
        match status {
            Ok(result) if result.success() => persist_install_state(&locale, &dir),
            Ok(result) => Err(format!("ERR_INSTALL_FAILED:{}", result.code().unwrap_or(-1))),
            Err(error) => Err(error),
        }
    }).await.map_err(|error| error.to_string())?
}

fn uninstall_dir() -> Result<PathBuf, String> {
    env::args().find_map(|argument| argument.strip_prefix("--install-dir=").map(PathBuf::from)).filter(|path| path.is_absolute()).ok_or_else(|| "ERR_UNINSTALL_DIRECTORY_UNKNOWN".into())
}

fn remove_file_with_retry(path: &Path) {
    for _ in 0..20 {
        if !path.exists() || fs::remove_file(path).is_ok() { break; }
        thread::sleep(Duration::from_millis(100));
    }
}

fn finish_uninstall_cleanup(dir: &Path, core: &Path) -> Result<(), String> {
    remove_file_with_retry(core);
    remove_file_with_retry(&dir.join("windows-client.exe"));
    remove_file_with_retry(&dir.join("uninstall.exe"));
    remove_file_with_retry(&dir.join("uninstall-core.exe"));
    remove_file_with_retry(&dir.join("uninstall-ui.exe"));
    let _ = fs::remove_file(dir.join("MyLIST.lnk"));
    let _ = fs::remove_dir_all(dir.join("docs"));
    let _ = fs::remove_dir_all(dir.join(".mylist"));
    let _ = fs::remove_dir(dir);
    remove_matching_install_location(dir);
    if core.exists() || dir.join("windows-client.exe").exists() { Err("ERR_UNINSTALL_FILES_BUSY".into()) } else { Ok(()) }
}

#[tauri::command]
async fn uninstall_mylist(delete_data: bool) -> Result<(), String> {
    let dir = uninstall_dir()?;
    let core = dir.join(".mylist").join("uninstall-core.exe");
    if !core.exists() { return Err("ERR_UNINSTALL_CORE_MISSING".into()); }
    tauri::async_runtime::spawn_blocking(move || {
        let mut command = Command::new(&core);
        command.arg("/S").arg("/P");
        if delete_data { command.arg("/WEBUI_DELETE_DATA"); }
        command.raw_arg(format!("_?={}", dir.display()));
        let status = command.status().map_err(|error| format!("ERR_START_UNINSTALLER:{error}"))?;
        if status.success() { finish_uninstall_cleanup(&dir, &core) }
        else { Err(format!("ERR_UNINSTALL_FAILED:{}", status.code().unwrap_or(-1))) }
    }).await.map_err(|error| error.to_string())?
}

#[tauri::command]
fn launch_mylist(directory: String) -> Result<(), String> {
    let executable = valid(&directory)?.join("windows-client.exe");
    if !executable.exists() { return Err("ERR_MAIN_MISSING".into()); }
    Command::new(executable).spawn().map(|_| ()).map_err(|error| format!("ERR_START_MAIN:{error}"))
}

fn bootstrap_uninstaller() -> Result<bool, String> {
    if BUILD_MODE != "uninstaller" || env::args().any(|argument| argument == "--temp-run") { return Ok(false); }
    let current = env::current_exe().map_err(|error| error.to_string())?;
    let install_dir = current.parent().ok_or("ERR_UNINSTALL_DIRECTORY_UNKNOWN")?.to_path_buf();
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_millis();
    let temp = env::temp_dir().join(format!("mylist-uninstaller-{stamp}.exe"));
    fs::copy(current, &temp).map_err(|error| format!("ERR_PREPARE_UNINSTALLER:{error}"))?;
    Command::new(temp).arg("--temp-run").arg(format!("--install-dir={}", install_dir.display())).spawn().map_err(|error| format!("ERR_START_UNINSTALLER:{error}"))?;
    Ok(true)
}

fn main() {
    if bootstrap_uninstaller().unwrap_or(false) { return; }
    tauri::Builder::default().invoke_handler(tauri::generate_handler![shell_mode, system_locale, default_install_directory, choose_install_directory, install_mylist, uninstall_mylist, launch_mylist]).run(tauri::generate_context!()).expect("installer shell failed");
}
