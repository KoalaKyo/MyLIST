//! Native labels are supplied by the renderer's shared locale catalog.
//! Locale source files are copied once to LocalAppData and read from there at
//! runtime, so translators do not need to rebuild the application. English is
//! the native safety fallback if the renderer or an external locale is broken.

use std::{collections::BTreeMap, fs};

use tauri::{AppHandle, Manager};

const SUPPORTED_LOCALES: [&str; 8] = ["zh-CN", "en", "de", "fr", "it", "es", "ja", "zh-TW"];

const LOCALE_SOURCES: [(&str, &str); 8] = [
    ("zh-CN", include_str!("../../src/i18n/zh-CN.ts")),
    ("en", include_str!("../../src/i18n/en.ts")),
    ("de", include_str!("../../src/i18n/de.ts")),
    ("fr", include_str!("../../src/i18n/fr.ts")),
    ("it", include_str!("../../src/i18n/it.ts")),
    ("es", include_str!("../../src/i18n/es.ts")),
    ("ja", include_str!("../../src/i18n/ja.ts")),
    ("zh-TW", include_str!("../../src/i18n/zh-TW.ts")),
];

fn locale_seed_source(locale: &str, source: &str) -> String {
    // Untranslated templates use a tiny `...zhCN.messages` re-export in the
    // source tree. External translator files must instead be independently
    // editable, complete catalogs. Seed them with the Chinese baseline and
    // only adjust harmless metadata; the parser consumes the message entries.
    if parse_message_lines(source)
        .map(|messages| messages.is_empty())
        .unwrap_or(false)
    {
        let chinese = LOCALE_SOURCES
            .iter()
            .find(|(known_locale, _)| *known_locale == "zh-CN")
            .map(|(_, value)| *value)
            .unwrap_or(source);
        return chinese.replace("locale: \"zh-CN\"", &format!("locale: \"{locale}\""));
    }
    source.to_string()
}

pub fn locale_directory(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let directory = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "locale_directory_unavailable".to_string())?
        .join("locales");
    fs::create_dir_all(&directory).map_err(|_| "locale_directory_unavailable".to_string())?;
    Ok(directory)
}

/// Never overwrite a user-edited language file. The application only seeds a
/// missing file from the bundled baseline.
pub fn ensure_external_locale_files(app: &AppHandle) -> Result<(), String> {
    let directory = locale_directory(app)?;
    for (locale, source) in LOCALE_SOURCES {
        let path = directory.join(format!("{locale}.ts"));
        let should_seed = !path.exists()
            || fs::read_to_string(&path)
                .ok()
                .and_then(|existing| parse_message_lines(&existing).ok())
                .map(|messages| messages.is_empty())
                .unwrap_or(false);
        if should_seed {
            fs::write(&path, locale_seed_source(locale, source))
                .map_err(|_| "locale_file_write_failed".to_string())?;
        }
    }
    Ok(())
}

pub fn read_external_messages(
    app: &AppHandle,
    locale: &str,
) -> Result<BTreeMap<String, String>, String> {
    if !SUPPORTED_LOCALES.contains(&locale) {
        return Err("locale_not_supported".to_string());
    }
    ensure_external_locale_files(app)?;
    let path = locale_directory(app)?.join(format!("{locale}.ts"));
    let source = fs::read_to_string(path).map_err(|_| "locale_file_read_failed".to_string())?;
    let messages = parse_message_lines(&source)?;
    // Untranslated starter files may deliberately re-export zh-CN while a
    // translator works on them. Treat that shape as the complete Chinese
    // baseline instead of exposing an empty interface.
    if messages.is_empty() && locale != "zh-CN" {
        let chinese_source = LOCALE_SOURCES
            .iter()
            .find(|(known_locale, _)| *known_locale == "zh-CN")
            .map(|(_, value)| *value)
            .ok_or_else(|| "locale_file_invalid".to_string())?;
        return parse_message_lines(chinese_source);
    }
    Ok(messages)
}

/// The external source keeps the same readable `"key": "value",` layout as
/// the project files. JSON decoding the right-hand side correctly handles
/// escaped quotes and line-break escapes without executing a user file.
fn parse_message_lines(source: &str) -> Result<BTreeMap<String, String>, String> {
    let mut messages = BTreeMap::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('"') {
            continue;
        }
        let Some((key_fragment, value_fragment)) = trimmed.split_once("\":") else {
            continue;
        };
        let key = key_fragment.trim_matches('"');
        let encoded_value = value_fragment.trim().trim_end_matches(',').trim();
        let value: String =
            serde_json::from_str(encoded_value).map_err(|_| "locale_file_invalid".to_string())?;
        messages.insert(key.to_string(), value);
    }
    Ok(messages)
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeLabels {
    pub open_main: String,
    pub show_desktop: String,
    pub topmost: String,
    pub normal: String,
    pub desktop: String,
    pub quit: String,
    pub plaintext_file: String,
    pub encrypted_file: String,
}

impl Default for NativeLabels {
    fn default() -> Self {
        Self {
            open_main: "Show main window".into(),
            show_desktop: "Show desktop".into(),
            topmost: "Always on top".into(),
            normal: "Normal mode".into(),
            desktop: "Desktop mode".into(),
            quit: "Quit".into(),
            plaintext_file: "MyLIST data".into(),
            encrypted_file: "MyLIST encrypted data".into(),
        }
    }
}
