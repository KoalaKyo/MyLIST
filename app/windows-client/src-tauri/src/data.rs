use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{Datelike, Months, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const DATABASE_FILE: &str = "mylist.sqlite3";
const INSTALLER_LOCALE_FILE: &str = ".installer-locale";
const THEME_KEY: &str = "theme";
const STARTUP_ENABLED_KEY: &str = "startup_enabled";
const LOCALE_KEY: &str = "locale";
const MCP_ENABLED_KEY: &str = "mcp_enabled";
const INTERFACE_TRANSPARENCY_KEY: &str = "interface_transparency";
const WINDOW_STATE_KEY: &str = "window_state";
const DEFAULT_LOCALE: &str = "zh-CN";

// Approved 3 × 8 palette: light / medium / dark.
const PALETTE: [[&str; 8]; 3] = [
    [
        "#D6E5FF", "#D6F1FF", "#D3F3E2", "#FFDCDB", "#FFECDB", "#FFF5CC", "#FBDBFF", "#FFDBEA",
    ],
    [
        "#8CB9FF", "#7DD8FF", "#77D6AC", "#FF9390", "#FFB774", "#FFE36A", "#D294F4", "#FF93C6",
    ],
    [
        "#1A4FBC", "#007DBB", "#2E7D52", "#A9282A", "#B95612", "#B58E00", "#6E219E", "#A72E6E",
    ],
];

/// Stable keys intentionally survive locale changes and data transfer.
/// The Chinese names are only canonical seed values for old databases and exports;
/// the renderer chooses the localized display label for records with a key.
const DEFAULT_CATEGORIES: [(&str, &str, usize); 8] = [
    ("personal", "个人", 7),
    ("team", "团队", 6),
    ("work", "工作", 0),
    ("life", "生活", 2),
    ("travel", "出行", 4),
    ("finance", "财务", 5),
    ("study", "学习", 1),
    ("other", "其他", 7),
];

#[derive(Debug)]
pub struct DataError(String);

impl std::fmt::Display for DataError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for DataError {}
impl From<rusqlite::Error> for DataError {
    fn from(error: rusqlite::Error) -> Self {
        Self(format!("本地数据操作失败：{error}"))
    }
}

/// Startup must never expose an untrusted SQLite error verbatim or imply that
/// recovery has changed the user's data. The caller can safely show this text
/// before the renderer and locale settings are available.
fn database_open_error(error: impl std::fmt::Display) -> DataError {
    let detail = error.to_string().to_ascii_lowercase();
    if detail.contains("not a database")
        || detail.contains("database disk image is malformed")
        || detail.contains("file is encrypted")
        || detail.contains("malformed")
    {
        return DataError(
            "本地数据文件无法验证，应用未修改任何数据。请先备份本地数据后再联系支持。".into(),
        );
    }

    DataError(
        "无法打开本地数据，应用未修改任何数据。请检查磁盘空间和本地目录访问权限后重试。".into(),
    )
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaletteColorDto {
    pub id: String,
    pub row: u8,
    pub column: u8,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDto {
    pub id: String,
    pub name: String,
    pub default_key: Option<String>,
    pub name_override: Option<String>,
    pub color_id: String,
    pub color: String,
    pub revision: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDeletePreviewDto {
    pub category: CategoryDto,
    pub task_count: i64,
    pub migration_targets: Vec<CategoryDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDeleteResultDto {
    pub id: String,
    pub deleted: bool,
    pub migrated_task_count: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapDto {
    pub device_id: String,
    pub theme: String,
    pub locale: String,
    pub startup_enabled: bool,
    pub mcp_enabled: bool,
    pub interface_transparency: u8,
    pub categories: Vec<CategoryDto>,
    pub palette: Vec<PaletteColorDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowStateDto {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub mode: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskInput {
    pub title: String,
    pub note: String,
    pub category_id: String,
    pub due_at_utc_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskInput {
    pub id: String,
    pub title: String,
    pub note: String,
    pub category_id: String,
    pub due_at_utc_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCategoryInput {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCategoryInput {
    pub id: String,
    pub name: String,
    pub color_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDto {
    pub id: String,
    pub title: String,
    pub note: String,
    pub category_id: String,
    pub category_name: String,
    pub category_default_key: Option<String>,
    pub category_name_override: Option<String>,
    pub category_color: String,
    pub status: String,
    pub due_at_utc_ms: Option<i64>,
    pub recurrence_json: Option<String>,
    pub created_at_utc_ms: i64,
    pub updated_at_utc_ms: i64,
    pub completed_at_utc_ms: Option<i64>,
    pub revision: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecurrenceConfig {
    pub interval: u16,
    pub unit: String,
    pub action: String,
    #[serde(default)]
    pub base_title: String,
}

/// Stable, device-independent business snapshot used by `.dtodo.json` exports.
/// Device settings intentionally remain local and are not part of this package.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaintextExportDto {
    pub schema_version: u32,
    pub export_id: String,
    pub exported_at_utc_ms: i64,
    pub source_device_id: String,
    pub palette: Vec<ExportPaletteColorDto>,
    pub categories: Vec<ExportCategoryDto>,
    pub tasks: Vec<ExportTaskDto>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPaletteColorDto {
    pub id: String,
    pub row: u8,
    pub column: u8,
    pub value: String,
    pub created_at_utc_ms: i64,
    pub updated_at_utc_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCategoryDto {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub default_key: Option<String>,
    #[serde(default)]
    pub name_override: Option<String>,
    pub color_id: String,
    pub created_at_utc_ms: i64,
    pub updated_at_utc_ms: i64,
    pub revision: i64,
    pub updated_by_device_id: String,
    pub sort_order: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTaskDto {
    pub id: String,
    pub title: String,
    pub note: String,
    pub category_id: String,
    pub status: String,
    pub due_at_utc_ms: Option<i64>,
    #[serde(default)]
    pub recurrence_json: Option<String>,
    pub completed_at_utc_ms: Option<i64>,
    pub created_at_utc_ms: i64,
    pub updated_at_utc_ms: i64,
    pub revision: i64,
    pub updated_by_device_id: String,
}

/// Read-only result for the first half of a merge import. The package is
/// validated before this structure is returned, so the UI never presents a
/// confirmation for malformed data.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewDto {
    pub session_id: String,
    pub source_file_name: String,
    pub source_device_id: String,
    pub exported_at_utc_ms: i64,
    pub task_count: usize,
    pub category_count: usize,
    pub palette_count: usize,
    pub new_tasks: usize,
    pub updated_tasks: usize,
    pub kept_tasks: usize,
    pub new_categories: usize,
    pub updated_categories: usize,
    pub kept_categories: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultDto {
    pub source_file_name: String,
    pub new_tasks: usize,
    pub updated_tasks: usize,
    pub kept_tasks: usize,
    pub new_categories: usize,
    pub updated_categories: usize,
    pub kept_categories: usize,
    pub snapshot_created: bool,
}

pub struct DataStore {
    connection: Mutex<Connection>,
    #[allow(dead_code)]
    database_path: PathBuf,
}

impl DataStore {
    pub fn open(app: &AppHandle) -> Result<Self, DataError> {
        let data_directory = app
            .path()
            .app_local_data_dir()
            .map_err(|error| DataError(format!("无法确定本地数据目录：{error}")))?;
        fs::create_dir_all(&data_directory)
            .map_err(|error| DataError(format!("无法创建本地数据目录：{error}")))?;
        let database_path = data_directory.join(DATABASE_FILE);
        let mut connection = Connection::open(&database_path).map_err(database_open_error)?;
        initialize_database(&mut connection).map_err(database_open_error)?;
        apply_installer_locale(&connection, &data_directory)?;
        Ok(Self {
            connection: Mutex::new(connection),
            database_path,
        })
    }

    pub fn bootstrap(&self) -> Result<BootstrapDto, DataError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        read_bootstrap(&connection)
    }

    pub fn write_plaintext_export(&self, path: &Path) -> Result<(), DataError> {
        let snapshot = self.plaintext_export_snapshot()?;
        let encoded = serde_json::to_vec_pretty(&snapshot)
            .map_err(|error| DataError(format!("无法生成导出数据：{error}")))?;
        fs::write(path, encoded).map_err(|error| DataError(format!("无法写入导出文件：{error}")))
    }

    pub fn write_encrypted_export(&self, path: &Path, password: &str) -> Result<(), DataError> {
        let snapshot = self.plaintext_export_snapshot()?;
        let encoded = crate::crypto::encrypt_export(&snapshot, password)
            .map_err(|error| DataError(error.to_string()))?;
        fs::write(path, encoded).map_err(|error| DataError(format!("无法写入导出文件：{error}")))
    }

    pub fn plaintext_export_snapshot(&self) -> Result<PlaintextExportDto, DataError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let source_device_id = connection.query_row(
            "SELECT value FROM app_metadata WHERE key = 'device_id'",
            [],
            |row| row.get(0),
        )?;
        let mut palette_statement = connection.prepare(
            "SELECT id, row_index, column_index, value, created_at_utc_ms, updated_at_utc_ms
             FROM palette_colors WHERE deleted_at_utc_ms IS NULL ORDER BY row_index, column_index, id",
        )?;
        let palette = palette_statement
            .query_map([], |row| {
                Ok(ExportPaletteColorDto {
                    id: row.get(0)?,
                    row: row.get(1)?,
                    column: row.get(2)?,
                    value: row.get(3)?,
                    created_at_utc_ms: row.get(4)?,
                    updated_at_utc_ms: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut category_statement = connection.prepare(
            "SELECT id, name, default_key, name_override, color_id, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id, sort_order
             FROM categories WHERE deleted_at_utc_ms IS NULL ORDER BY sort_order, created_at_utc_ms, id",
        )?;
        let categories = category_statement
            .query_map([], |row| {
                Ok(ExportCategoryDto {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    default_key: row.get(2)?,
                    name_override: row.get(3)?,
                    color_id: row.get(4)?,
                    created_at_utc_ms: row.get(5)?,
                    updated_at_utc_ms: row.get(6)?,
                    revision: row.get(7)?,
                    updated_by_device_id: row.get(8)?,
                    sort_order: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut task_statement = connection.prepare(
            "SELECT id, title, note, category_id, status, due_at_utc_ms, recurrence_json, completed_at_utc_ms, created_at_utc_ms, updated_at_utc_ms,
                    revision, updated_by_device_id
             FROM tasks WHERE deleted_at_utc_ms IS NULL ORDER BY created_at_utc_ms, id",
        )?;
        let tasks = task_statement
            .query_map([], |row| {
                Ok(ExportTaskDto {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    note: row.get(2)?,
                    category_id: row.get(3)?,
                    status: row.get(4)?,
                    due_at_utc_ms: row.get(5)?,
                    recurrence_json: row.get(6)?,
                    completed_at_utc_ms: row.get(7)?,
                    created_at_utc_ms: row.get(8)?,
                    updated_at_utc_ms: row.get(9)?,
                    revision: row.get(10)?,
                    updated_by_device_id: row.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(PlaintextExportDto {
            schema_version: 1,
            export_id: Uuid::new_v4().to_string(),
            exported_at_utc_ms: utc_now_ms(),
            source_device_id,
            palette,
            categories,
            tasks,
        })
    }

    pub fn read_plaintext_import_bytes(
        &self,
        encoded: &[u8],
        file_name: String,
    ) -> Result<(PlaintextExportDto, String), DataError> {
        let package: PlaintextExportDto = serde_json::from_slice(encoded)
            .map_err(|_| DataError("导入文件格式无效或已损坏".into()))?;
        validate_import_package(&package)?;
        Ok((package, file_name))
    }

    pub fn preview_import_package(
        &self,
        package: &PlaintextExportDto,
        source_file_name: &str,
    ) -> Result<ImportPreviewDto, DataError> {
        validate_import_package(package)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        build_import_preview(&connection, package, source_file_name)
    }

    pub fn import_plaintext_package(
        &self,
        package: &PlaintextExportDto,
        source_file_name: &str,
    ) -> Result<ImportResultDto, DataError> {
        validate_import_package(package)?;
        self.create_import_snapshot(source_file_name)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = merge_plaintext_package(&transaction, package, source_file_name)?;
        transaction.commit()?;
        Ok(result)
    }

    /// Replaces only exported business entities. Device settings and the import
    /// history remain local to this Windows installation.
    pub fn replace_plaintext_package(
        &self,
        package: &PlaintextExportDto,
        source_file_name: &str,
    ) -> Result<ImportResultDto, DataError> {
        validate_import_package(package)?;
        self.create_import_snapshot(source_file_name)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute("DELETE FROM tasks", [])?;
        transaction.execute("DELETE FROM categories", [])?;
        transaction.execute("DELETE FROM palette_colors", [])?;
        for color in &package.palette {
            transaction.execute(
                "INSERT INTO palette_colors (id, row_index, column_index, value, created_at_utc_ms, updated_at_utc_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![color.id, color.row, color.column, color.value, color.created_at_utc_ms, color.updated_at_utc_ms],
            )?;
        }
        for category in &package.categories {
            transaction.execute(
            "INSERT INTO categories (id, name, default_key, name_override, color_id, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![category.id, validate_category_name(&category.name)?, category.default_key, category.name_override, category.color_id, category.created_at_utc_ms, category.updated_at_utc_ms, category.revision, category.updated_by_device_id, category.sort_order],
            )?;
        }
        for task in &package.tasks {
            transaction.execute(
                "INSERT INTO tasks (id, title, note, category_id, status, due_at_utc_ms, recurrence_json, completed_at_utc_ms, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![task.id, task.title.trim(), task.note.trim(), task.category_id, task.status, task.due_at_utc_ms, task.recurrence_json, task.completed_at_utc_ms, task.created_at_utc_ms, task.updated_at_utc_ms, task.revision, task.updated_by_device_id],
            )?;
        }
        transaction.commit()?;
        Ok(ImportResultDto {
            source_file_name: source_file_name.to_string(),
            new_tasks: package.tasks.len(),
            updated_tasks: 0,
            kept_tasks: 0,
            new_categories: package.categories.len(),
            updated_categories: 0,
            kept_categories: 0,
            snapshot_created: true,
        })
    }

    fn create_import_snapshot(&self, source_file_name: &str) -> Result<(), DataError> {
        let snapshot = self.plaintext_export_snapshot()?;
        let snapshot_id = Uuid::new_v4().to_string();
        let directory = self
            .database_path
            .parent()
            .ok_or_else(|| DataError("本机数据目录不可用".into()))?
            .join("import-snapshots");
        fs::create_dir_all(&directory)
            .map_err(|error| DataError(format!("无法创建导入快照：{error}")))?;
        let snapshot_path = directory.join(format!(
            "pre-import-{}-{snapshot_id}.dtodo.json",
            utc_now_ms()
        ));
        let temporary_path = snapshot_path.with_extension("tmp");
        let encoded = serde_json::to_vec_pretty(&snapshot)
            .map_err(|error| DataError(format!("无法创建导入快照：{error}")))?;
        fs::write(&temporary_path, encoded)
            .map_err(|error| DataError(format!("无法创建导入快照：{error}")))?;
        fs::rename(&temporary_path, &snapshot_path)
            .map_err(|error| DataError(format!("无法完成导入快照：{error}")))?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        if let Err(error) = connection.execute(
            "INSERT INTO import_snapshots (id, created_at_utc_ms, source_file_name, snapshot_path) VALUES (?1, ?2, ?3, ?4)",
            params![snapshot_id, utc_now_ms(), source_file_name, snapshot_path.to_string_lossy()],
        ) {
            let _ = fs::remove_file(&snapshot_path);
            return Err(DataError(format!("无法登记导入快照：{error}")));
        }
        Ok(())
    }

    pub fn save_theme(&self, theme: &str) -> Result<String, DataError> {
        if !matches!(theme, "light" | "dark") {
            return Err(DataError("不支持的主题设置".into()));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        connection.execute(
            "INSERT INTO device_settings (key, value, updated_at_utc_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at_utc_ms = excluded.updated_at_utc_ms",
            params![THEME_KEY, theme, utc_now_ms()],
        )?;
        Ok(theme.to_string())
    }

    pub fn save_interface_transparency(&self, transparency: u8) -> Result<u8, DataError> {
        if transparency > 30 || transparency % 5 != 0 {
            return Err(DataError("不支持的界面透明度设置".into()));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        connection.execute(
            "INSERT INTO device_settings (key, value, updated_at_utc_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at_utc_ms = excluded.updated_at_utc_ms",
            params![INTERFACE_TRANSPARENCY_KEY, transparency.to_string(), utc_now_ms()],
        )?;
        Ok(transparency)
    }

    pub fn save_locale(&self, locale: &str) -> Result<String, DataError> {
        if !matches!(
            locale,
            "zh-CN" | "en" | "de" | "fr" | "it" | "es" | "ja" | "zh-TW"
        ) {
            return Err(DataError("不支持的语言设置".into()));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        connection.execute(
            "INSERT INTO device_settings (key, value, updated_at_utc_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at_utc_ms = excluded.updated_at_utc_ms",
            params![LOCALE_KEY, locale, utc_now_ms()],
        )?;
        Ok(locale.to_string())
    }

    pub fn startup_enabled(&self) -> Result<bool, DataError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        read_startup_enabled(&connection)
    }

    pub fn save_startup_enabled(&self, enabled: bool) -> Result<bool, DataError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        connection.execute(
            "INSERT INTO device_settings (key, value, updated_at_utc_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at_utc_ms = excluded.updated_at_utc_ms",
            params![STARTUP_ENABLED_KEY, if enabled { "true" } else { "false" }, utc_now_ms()],
        )?;
        Ok(enabled)
    }

    pub fn mcp_enabled(&self) -> Result<bool, DataError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        read_mcp_enabled(&connection)
    }

    pub fn save_mcp_enabled(&self, enabled: bool) -> Result<bool, DataError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        connection.execute(
            "INSERT INTO device_settings (key, value, updated_at_utc_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at_utc_ms = excluded.updated_at_utc_ms",
            params![MCP_ENABLED_KEY, if enabled { "true" } else { "false" }, utc_now_ms()],
        )?;
        Ok(enabled)
    }

    pub fn window_state(&self) -> Result<Option<WindowStateDto>, DataError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let encoded = connection
            .query_row(
                "SELECT value FROM device_settings WHERE key = ?1",
                params![WINDOW_STATE_KEY],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        encoded
            .map(|value| {
                serde_json::from_str(&value).map_err(|_| DataError("窗口状态无法读取".into()))
            })
            .transpose()
    }

    pub fn save_window_state(&self, state: &WindowStateDto) -> Result<(), DataError> {
        let encoded = serde_json::to_string(state)
            .map_err(|error| DataError(format!("窗口状态无法保存：{error}")))?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        connection.execute(
            "INSERT INTO device_settings (key, value, updated_at_utc_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at_utc_ms = excluded.updated_at_utc_ms",
            params![WINDOW_STATE_KEY, encoded, utc_now_ms()],
        )?;
        Ok(())
    }

    pub fn create_category(&self, input: CreateCategoryInput) -> Result<CategoryDto, DataError> {
        let requested_name = validate_category_name(&input.name)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let name = next_available_category_name(&transaction, &requested_name)?;
        let color_id = next_available_color_id(&transaction)?;
        let now = utc_now_ms();
        let device_id = read_device_id(&transaction)?;
        let id = Uuid::new_v4().to_string();
        let sort_order: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM categories WHERE deleted_at_utc_ms IS NULL",
            [],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO categories (id, name, default_key, name_override, color_id, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id, sort_order)
             VALUES (?1, ?2, NULL, NULL, ?3, ?4, ?4, 1, ?5, ?6)",
            params![id, name, color_id, now, device_id, sort_order],
        )?;
        transaction.commit()?;
        read_category(&connection, &id)
    }

    pub fn update_category(&self, input: UpdateCategoryInput) -> Result<CategoryDto, DataError> {
        let name = validate_category_name(&input.name)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_active_category(&transaction, &input.id)?;
        ensure_palette_color(&transaction, &input.color_id)?;
        ensure_category_name_available(&transaction, &name, Some(&input.id))?;
        let changed = transaction.execute(
            "UPDATE categories SET
               name = CASE WHEN default_key IS NULL THEN ?1 ELSE name END,
               name_override = CASE WHEN default_key IS NULL OR ?1 = name THEN name_override ELSE ?1 END,
               color_id = ?2, updated_at_utc_ms = ?3, revision = revision + 1,
               updated_by_device_id = ?4 WHERE id = ?5 AND deleted_at_utc_ms IS NULL",
            params![name, input.color_id, utc_now_ms(), read_device_id(&transaction)?, input.id],
        )?;
        if changed != 1 {
            return Err(DataError("分类不存在或已删除".into()));
        }
        transaction.commit()?;
        read_category(&connection, &input.id)
    }

    pub fn delete_category(
        &self,
        id: &str,
        target_category_id: Option<&str>,
    ) -> Result<(), DataError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_active_category(&transaction, id)?;
        let task_count: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM tasks WHERE category_id = ?1 AND deleted_at_utc_ms IS NULL",
            params![id],
            |row| row.get(0),
        )?;
        let historical_task_count: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM tasks WHERE category_id = ?1 AND deleted_at_utc_ms IS NOT NULL",
            params![id],
            |row| row.get(0),
        )?;
        let fallback_target = if task_count == 0 && historical_task_count > 0 {
            Some(transaction
                .query_row(
                    "SELECT id FROM categories WHERE id <> ?1 AND deleted_at_utc_ms IS NULL ORDER BY sort_order, id LIMIT 1",
                    params![id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| DataError("至少保留一个分类后才能删除当前分类".into()))?)
        } else {
            None
        };
        let migration_target = if task_count > 0 {
            let target = target_category_id.ok_or_else(|| {
                DataError(format!(
                    "该分类仍有 {task_count} 个事项，请先选择迁移目标分类"
                ))
            })?;
            if target == id {
                return Err(DataError("迁移目标不能是当前分类".into()));
            }
            ensure_active_category(&transaction, target)?;
            Some(target.to_string())
        } else {
            fallback_target
        };
        if let Some(target) = migration_target {
            let now = utc_now_ms();
            let device_id = read_device_id(&transaction)?;
            transaction.execute(
                "UPDATE tasks SET category_id = ?1, updated_at_utc_ms = ?2, revision = revision + 1,
                 updated_by_device_id = ?3 WHERE category_id = ?4",
                params![target, now, device_id, id],
            )?;
        }
        let changed = transaction.execute(
            "DELETE FROM categories WHERE id = ?1 AND deleted_at_utc_ms IS NULL",
            params![id],
        )?;
        if changed != 1 {
            return Err(DataError("分类不存在或已删除".into()));
        }
        transaction.commit()?;
        Ok(())
    }

    /// Restores the eight built-in categories without discarding a user's renamed
    /// default category. A renamed default is converted into a normal category
    /// first, its tasks follow it, and the original stable default identity is
    /// reset so it can render in the current locale again.
    pub fn restore_default_categories(&self) -> Result<(), DataError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = utc_now_ms();
        let device_id = read_device_id(&transaction)?;
        let next_sort_order: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM categories WHERE deleted_at_utc_ms IS NULL",
            [],
            |row| row.get(0),
        )?;
        let mut added = 0_i64;

        for (key, name, color_column) in DEFAULT_CATEGORIES {
            let category = transaction
                .query_row(
                    "SELECT id, name, name_override, color_id FROM categories
                     WHERE default_key = ?1 AND deleted_at_utc_ms IS NULL
                     LIMIT 1",
                    params![key],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )
                .optional()?;
            match category {
                Some((id, canonical_name, Some(override_name), color_id)) => {
                    let custom_name = next_available_category_name(&transaction, &override_name)?;
                    let custom_id = Uuid::new_v4().to_string();
                    transaction.execute(
                        "INSERT INTO categories (id, name, default_key, name_override, color_id, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id, sort_order)
                         VALUES (?1, ?2, NULL, NULL, ?3, ?4, ?4, 1, ?5, ?6)",
                        params![custom_id, custom_name, color_id, now, device_id, next_sort_order + added],
                    )?;
                    transaction.execute(
                        "UPDATE tasks SET category_id = ?1, updated_at_utc_ms = ?2, revision = revision + 1, updated_by_device_id = ?3
                         WHERE category_id = ?4",
                        params![custom_id, now, device_id, id],
                    )?;
                    transaction.execute(
                        "UPDATE categories SET name = ?1, name_override = NULL, color_id = ?2,
                         updated_at_utc_ms = ?3, revision = revision + 1, updated_by_device_id = ?4
                         WHERE id = ?5",
                        params![
                            canonical_name,
                            palette_id(1, color_column),
                            now,
                            device_id,
                            id
                        ],
                    )?;
                    added += 1;
                }
                Some(_) => {}
                None => {
                    let same_name = transaction
                        .query_row(
                            "SELECT id FROM categories WHERE name = ?1 AND deleted_at_utc_ms IS NULL LIMIT 1",
                            params![name],
                            |row| row.get::<_, String>(0),
                        )
                        .optional()?;
                    if let Some(id) = same_name {
                        // A legacy database may still have the canonical Chinese
                        // name without a stable key. Upgrade that record in place.
                        transaction.execute(
                            "UPDATE categories SET default_key = ?1, name_override = NULL,
                             updated_at_utc_ms = ?2, revision = revision + 1, updated_by_device_id = ?3
                             WHERE id = ?4",
                            params![key, now, device_id, id],
                        )?;
                    } else {
                        transaction.execute(
                            "INSERT INTO categories (id, name, default_key, name_override, color_id, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id, sort_order)
                             VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?5, 1, ?6, ?7)",
                            params![Uuid::new_v4().to_string(), name, key, palette_id(1, color_column), now, device_id, next_sort_order + added],
                        )?;
                        added += 1;
                    }
                }
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn list_tasks(&self, status: &str) -> Result<Vec<TaskDto>, DataError> {
        validate_status(status)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        read_tasks(&connection, status)
    }

    pub fn get_task(&self, id: &str) -> Result<TaskDto, DataError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        read_task(&connection, id)
    }

    pub fn create_task(&self, input: CreateTaskInput) -> Result<TaskDto, DataError> {
        let title = validate_task_input(&input.title, &input.note)?;
        validate_due_at(input.due_at_utc_ms)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_active_category(&transaction, &input.category_id)?;
        let now = utc_now_ms();
        let device_id = read_device_id(&transaction)?;
        let id = Uuid::new_v4().to_string();
        transaction.execute(
            "INSERT INTO tasks (id, title, note, category_id, status, due_at_utc_ms, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id)
             VALUES (?1, ?2, ?3, ?4, 'todo', ?5, ?6, ?6, 1, ?7)",
            params![id, title, input.note.trim(), input.category_id, input.due_at_utc_ms, now, device_id],
        )?;
        transaction.commit()?;
        read_task(&connection, &id)
    }

    /// MCP write operations are idempotent within this installation. The
    /// request id and final task snapshot live in the same SQLite transaction
    /// as the mutation, so a retried agent request never creates a duplicate.
    pub fn mcp_create_task(
        &self,
        request_id: &str,
        input: CreateTaskInput,
        recurrence: Option<RecurrenceConfig>,
    ) -> Result<TaskDto, DataError> {
        let title = validate_task_input(&input.title, &input.note)?;
        validate_due_at(input.due_at_utc_ms)?;
        if let Some(config) = &recurrence {
            validate_recurrence(config)?;
            if input.due_at_utc_ms.is_none() {
                return Err(DataError("请先设置截止时间，再开启重复事项".into()));
            }
        }
        let recurrence_json = recurrence
            .map(|config| {
                serde_json::to_string(&config).map_err(|error| DataError(error.to_string()))
            })
            .transpose()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(task) = read_mcp_request_task(&transaction, request_id, "create")? {
            return Ok(task);
        }
        ensure_active_category(&transaction, &input.category_id)?;
        let now = utc_now_ms();
        let id = Uuid::new_v4().to_string();
        transaction.execute("INSERT INTO tasks (id, title, note, category_id, status, due_at_utc_ms, recurrence_json, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id) VALUES (?1, ?2, ?3, ?4, 'todo', ?5, ?6, ?7, ?7, 1, ?8)", params![id, title, input.note.trim(), input.category_id, input.due_at_utc_ms, recurrence_json, now, read_device_id(&transaction)?])?;
        let task = read_task_transaction(&transaction, &id)?;
        save_mcp_request_task(&transaction, request_id, "create", &task)?;
        transaction.commit()?;
        Ok(task)
    }

    pub fn mcp_update_task(
        &self,
        request_id: &str,
        input: UpdateTaskInput,
        expected_revision: i64,
        recurrence: Option<Option<RecurrenceConfig>>,
    ) -> Result<TaskDto, DataError> {
        let title = validate_task_input(&input.title, &input.note)?;
        validate_due_at(input.due_at_utc_ms)?;
        if let Some(Some(config)) = &recurrence {
            validate_recurrence(config)?;
            if input.due_at_utc_ms.is_none() {
                return Err(DataError("请先设置截止时间，再开启重复事项".into()));
            }
        }
        let recurrence_json = recurrence
            .map(|value| {
                value
                    .map(|config| {
                        serde_json::to_string(&config).map_err(|error| DataError(error.to_string()))
                    })
                    .transpose()
            })
            .transpose()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(task) = read_mcp_request_task(&transaction, request_id, "update")? {
            return Ok(task);
        }
        ensure_active_category(&transaction, &input.category_id)?;
        let changed = if let Some(recurrence_json) = recurrence_json {
            transaction.execute("UPDATE tasks SET title=?1, note=?2, category_id=?3, due_at_utc_ms=?4, recurrence_json=?5, updated_at_utc_ms=?6, revision=revision+1, updated_by_device_id=?7 WHERE id=?8 AND revision=?9 AND deleted_at_utc_ms IS NULL", params![title, input.note.trim(), input.category_id, input.due_at_utc_ms, recurrence_json, utc_now_ms(), read_device_id(&transaction)?, input.id, expected_revision])?
        } else {
            transaction.execute("UPDATE tasks SET title=?1, note=?2, category_id=?3, due_at_utc_ms=?4, updated_at_utc_ms=?5, revision=revision+1, updated_by_device_id=?6 WHERE id=?7 AND revision=?8 AND deleted_at_utc_ms IS NULL", params![title, input.note.trim(), input.category_id, input.due_at_utc_ms, utc_now_ms(), read_device_id(&transaction)?, input.id, expected_revision])?
        };
        if changed != 1 {
            return Err(DataError("事项已被更新，请先重新读取后再编辑".into()));
        }
        let task = read_task_transaction(&transaction, &input.id)?;
        save_mcp_request_task(&transaction, request_id, "update", &task)?;
        transaction.commit()?;
        Ok(task)
    }

    pub fn mcp_set_task_status(
        &self,
        request_id: &str,
        id: &str,
        status: &str,
        expected_revision: i64,
    ) -> Result<TaskDto, DataError> {
        validate_status(status)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(task) = read_mcp_request_task(&transaction, request_id, status)? {
            return Ok(task);
        }
        let now = utc_now_ms();
        let completed_at = if status == "completed" {
            Some(now)
        } else {
            None
        };
        let changed = transaction.execute("UPDATE tasks SET status=?1, completed_at_utc_ms=?2, updated_at_utc_ms=?3, revision=revision+1, updated_by_device_id=?4 WHERE id=?5 AND revision=?6 AND deleted_at_utc_ms IS NULL", params![status, completed_at, now, read_device_id(&transaction)?, id, expected_revision])?;
        if changed != 1 {
            return Err(DataError("事项已被更新，请先重新读取后再操作".into()));
        }
        let task = read_task_transaction(&transaction, id)?;
        save_mcp_request_task(&transaction, request_id, status, &task)?;
        transaction.commit()?;
        Ok(task)
    }

    /// Category MCP writes use the same request log as task writes.  This
    /// makes a retried agent request safe without exposing the database to the
    /// bridge process.
    pub fn mcp_create_category(
        &self,
        request_id: &str,
        name: &str,
        color_id: Option<&str>,
    ) -> Result<CategoryDto, DataError> {
        let requested_name = validate_category_name(name)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(category) =
            read_mcp_request_category(&transaction, request_id, "create-category")?
        {
            return Ok(category);
        }
        let name = next_available_category_name(&transaction, &requested_name)?;
        let color_id = match color_id.filter(|value| !value.trim().is_empty()) {
            Some(value) => {
                ensure_palette_color(&transaction, value)?;
                value.to_string()
            }
            None => next_available_color_id(&transaction)?,
        };
        let id = Uuid::new_v4().to_string();
        let now = utc_now_ms();
        let sort_order: i64 = transaction.query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM categories WHERE deleted_at_utc_ms IS NULL",
            [],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO categories (id, name, default_key, name_override, color_id, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id, sort_order)
             VALUES (?1, ?2, NULL, NULL, ?3, ?4, ?4, 1, ?5, ?6)",
            params![id, name, color_id, now, read_device_id(&transaction)?, sort_order],
        )?;
        let category = read_category_transaction(&transaction, &id)?;
        save_mcp_request_category(&transaction, request_id, "create-category", &category)?;
        transaction.commit()?;
        Ok(category)
    }

    pub fn mcp_update_category(
        &self,
        request_id: &str,
        input: UpdateCategoryInput,
        expected_revision: i64,
    ) -> Result<CategoryDto, DataError> {
        let name = validate_category_name(&input.name)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(category) =
            read_mcp_request_category(&transaction, request_id, "update-category")?
        {
            return Ok(category);
        }
        ensure_palette_color(&transaction, &input.color_id)?;
        ensure_category_name_available(&transaction, &name, Some(&input.id))?;
        let changed = transaction.execute(
            "UPDATE categories SET
               name = CASE WHEN default_key IS NULL THEN ?1 ELSE name END,
               name_override = CASE WHEN default_key IS NULL OR ?1 = name THEN name_override ELSE ?1 END,
               color_id = ?2, updated_at_utc_ms = ?3, revision = revision + 1,
               updated_by_device_id = ?4
             WHERE id = ?5 AND revision = ?6 AND deleted_at_utc_ms IS NULL",
            params![name, input.color_id, utc_now_ms(), read_device_id(&transaction)?, input.id, expected_revision],
        )?;
        if changed != 1 {
            return Err(DataError("分类已被更新，请先重新读取后再编辑".into()));
        }
        let category = read_category_transaction(&transaction, &input.id)?;
        save_mcp_request_category(&transaction, request_id, "update-category", &category)?;
        transaction.commit()?;
        Ok(category)
    }

    /// A delete preflight deliberately performs no mutation.  AI-5 will bind
    /// this preview to a local confirmation token before exposing deletion.
    pub fn mcp_prepare_delete_category(
        &self,
        id: &str,
    ) -> Result<CategoryDeletePreviewDto, DataError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let category = read_category(&connection, id)?;
        let task_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM tasks WHERE category_id = ?1 AND deleted_at_utc_ms IS NULL",
            params![id],
            |row| row.get(0),
        )?;
        let mut statement = connection.prepare(
            "SELECT categories.id, categories.name, categories.default_key, categories.name_override, categories.color_id, palette_colors.value, categories.revision
             FROM categories JOIN palette_colors ON palette_colors.id = categories.color_id
             WHERE categories.id <> ?1 AND categories.deleted_at_utc_ms IS NULL
             ORDER BY categories.sort_order, categories.created_at_utc_ms, categories.id",
        )?;
        let migration_targets = statement
            .query_map(params![id], |row| {
                Ok(CategoryDto {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    default_key: row.get(2)?,
                    name_override: row.get(3)?,
                    color_id: row.get(4)?,
                    color: row.get(5)?,
                    revision: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CategoryDeletePreviewDto {
            category,
            task_count,
            migration_targets,
        })
    }

    pub fn mcp_prepare_delete_task(&self, id: &str) -> Result<TaskDto, DataError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        read_task(&connection, id)
    }

    pub fn mcp_delete_task(
        &self,
        request_id: &str,
        id: &str,
        expected_revision: i64,
    ) -> Result<McpDeleteResultDto, DataError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(result) = read_mcp_delete_result(&transaction, request_id, "delete-task")? {
            return Ok(result);
        }
        let changed = transaction.execute(
            "DELETE FROM tasks WHERE id = ?1 AND revision = ?2 AND deleted_at_utc_ms IS NULL",
            params![id, expected_revision],
        )?;
        if changed != 1 {
            return Err(DataError("事项已被更新，请先重新读取后再删除".into()));
        }
        let result = McpDeleteResultDto {
            id: id.to_string(),
            deleted: true,
            migrated_task_count: 0,
        };
        save_mcp_delete_result(&transaction, request_id, "delete-task", &result)?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn mcp_delete_category(
        &self,
        request_id: &str,
        id: &str,
        expected_revision: i64,
        target_category_id: Option<&str>,
    ) -> Result<McpDeleteResultDto, DataError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(result) = read_mcp_delete_result(&transaction, request_id, "delete-category")? {
            return Ok(result);
        }
        ensure_active_category(&transaction, id)?;
        let current_revision: i64 = transaction.query_row(
            "SELECT revision FROM categories WHERE id = ?1 AND deleted_at_utc_ms IS NULL",
            params![id],
            |row| row.get(0),
        )?;
        if current_revision != expected_revision {
            return Err(DataError("分类已被更新，请先重新读取后再删除".into()));
        }
        let task_count: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM tasks WHERE category_id = ?1 AND deleted_at_utc_ms IS NULL",
            params![id],
            |row| row.get(0),
        )?;
        let target = if task_count > 0 {
            let target = target_category_id.ok_or_else(|| {
                DataError(format!(
                    "该分类仍有 {task_count} 个事项，请先选择迁移目标分类"
                ))
            })?;
            if target == id {
                return Err(DataError("迁移目标不能是当前分类".into()));
            }
            ensure_active_category(&transaction, target)?;
            Some(target)
        } else {
            None
        };
        if let Some(target) = target {
            transaction.execute("UPDATE tasks SET category_id = ?1, updated_at_utc_ms = ?2, revision = revision + 1, updated_by_device_id = ?3 WHERE category_id = ?4 AND deleted_at_utc_ms IS NULL", params![target, utc_now_ms(), read_device_id(&transaction)?, id])?;
        }
        let changed = transaction.execute(
            "DELETE FROM categories WHERE id = ?1 AND revision = ?2 AND deleted_at_utc_ms IS NULL",
            params![id, expected_revision],
        )?;
        if changed != 1 {
            return Err(DataError("分类已被更新，请先重新读取后再删除".into()));
        }
        let result = McpDeleteResultDto {
            id: id.to_string(),
            deleted: true,
            migrated_task_count: task_count,
        };
        save_mcp_delete_result(&transaction, request_id, "delete-category", &result)?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn update_task(&self, input: UpdateTaskInput) -> Result<TaskDto, DataError> {
        let title = validate_task_input(&input.title, &input.note)?;
        validate_due_at(input.due_at_utc_ms)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_active_category(&transaction, &input.category_id)?;
        let device_id = read_device_id(&transaction)?;
        let changed = transaction.execute(
            "UPDATE tasks SET title = ?1, note = ?2, category_id = ?3, due_at_utc_ms = ?4, updated_at_utc_ms = ?5,
             revision = revision + 1, updated_by_device_id = ?6 WHERE id = ?7 AND deleted_at_utc_ms IS NULL",
            params![title, input.note.trim(), input.category_id, input.due_at_utc_ms, utc_now_ms(), device_id, input.id],
        )?;
        if changed != 1 {
            return Err(DataError("事项不存在或已删除".into()));
        }
        transaction.commit()?;
        read_task(&connection, &input.id)
    }

    pub fn set_task_status(&self, id: &str, status: &str) -> Result<TaskDto, DataError> {
        validate_status(status)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let device_id = read_device_id(&transaction)?;
        let now = utc_now_ms();
        let completed_at = if status == "completed" {
            Some(now)
        } else {
            None
        };
        let changed = transaction.execute(
            "UPDATE tasks SET status = ?1, completed_at_utc_ms = ?2, updated_at_utc_ms = ?3,
             revision = revision + 1, updated_by_device_id = ?4 WHERE id = ?5 AND deleted_at_utc_ms IS NULL",
            params![status, completed_at, now, device_id, id],
        )?;
        if changed != 1 {
            return Err(DataError("事项不存在或已删除".into()));
        }
        transaction.commit()?;
        read_task(&connection, id)
    }

    pub fn save_task_recurrence(
        &self,
        id: &str,
        recurrence: Option<RecurrenceConfig>,
    ) -> Result<TaskDto, DataError> {
        if let Some(config) = &recurrence {
            validate_recurrence(config)?;
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let due_at: Option<i64> = transaction
            .query_row(
                "SELECT due_at_utc_ms FROM tasks WHERE id=?1 AND deleted_at_utc_ms IS NULL",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        if recurrence.is_some() && due_at.is_none() {
            return Err(DataError("请先设置截止时间，再开启重复事项".into()));
        }
        let value = recurrence
            .map(|config| {
                serde_json::to_string(&config).map_err(|error| DataError(error.to_string()))
            })
            .transpose()?;
        let changed = transaction.execute("UPDATE tasks SET recurrence_json=?1, updated_at_utc_ms=?2, revision=revision+1, updated_by_device_id=?3 WHERE id=?4 AND deleted_at_utc_ms IS NULL", params![value, utc_now_ms(), read_device_id(&transaction)?, id])?;
        if changed != 1 {
            return Err(DataError("事项不存在或已删除".into()));
        }
        let task = read_task_transaction(&transaction, id)?;
        transaction.commit()?;
        Ok(task)
    }

    pub fn settle_due_recurrences(&self) -> Result<usize, DataError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let now = utc_now_ms();
        let mut statement = transaction.prepare("SELECT id,title,note,category_id,due_at_utc_ms,recurrence_json FROM tasks WHERE status='todo' AND deleted_at_utc_ms IS NULL AND due_at_utc_ms IS NOT NULL AND due_at_utc_ms <= ?1 AND recurrence_json IS NOT NULL")?;
        let rows = statement
            .query_map(params![now], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let device_id = read_device_id(&transaction)?;
        let mut settled = 0;
        for (id, title, note, category_id, due, json) in rows {
            let Ok(config) = serde_json::from_str::<RecurrenceConfig>(&json) else {
                continue;
            };
            if validate_recurrence(&config).is_err() {
                continue;
            }
            let next = next_recurrence_due(due, &config)?;
            if config.action == "update_due" {
                transaction.execute("UPDATE tasks SET due_at_utc_ms=?1, updated_at_utc_ms=?2, revision=revision+1, updated_by_device_id=?3 WHERE id=?4", params![next, now, device_id, id])?;
            } else {
                transaction.execute("UPDATE tasks SET status='completed', completed_at_utc_ms=?1, recurrence_json=NULL, updated_at_utc_ms=?1, revision=revision+1, updated_by_device_id=?2 WHERE id=?3", params![now, device_id, id])?;
                let base = if config.base_title.trim().is_empty() {
                    title
                } else {
                    config.base_title.clone()
                };
                let new_id = Uuid::new_v4().to_string();
                transaction.execute("INSERT INTO tasks (id,title,note,category_id,status,due_at_utc_ms,recurrence_json,created_at_utc_ms,updated_at_utc_ms,revision,updated_by_device_id) VALUES (?1,?2,?3,?4,'todo',?5,?6,?7,?7,1,?8)", params![new_id, recurrence_title(&base, next, &config), note, category_id, next, json, now, device_id])?;
            }
            settled += 1;
        }
        transaction.commit()?;
        Ok(settled)
    }

    pub fn delete_task(&self, id: &str) -> Result<(), DataError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "DELETE FROM tasks WHERE id = ?1 AND deleted_at_utc_ms IS NULL",
            params![id],
        )?;
        if changed != 1 {
            return Err(DataError("事项不存在或已删除".into()));
        }
        transaction.commit()?;
        Ok(())
    }
}

fn apply_installer_locale(connection: &Connection, data_directory: &Path) -> Result<(), DataError> {
    let marker = data_directory.join(INSTALLER_LOCALE_FILE);
    let Ok(locale) = fs::read_to_string(&marker) else { return Ok(()); };
    let locale = locale.trim();
    if !matches!(locale, "zh-CN" | "zh-TW" | "en" | "de" | "fr" | "it" | "es" | "ja") { return Ok(()); }
    connection.execute(
        "INSERT INTO device_settings (key, value, updated_at_utc_ms) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at_utc_ms = excluded.updated_at_utc_ms",
        params![LOCALE_KEY, locale, utc_now_ms()],
    ).map_err(|error| DataError(format!("无法应用安装语言：{error}")))?;
    let _ = fs::remove_file(marker);
    Ok(())
}

fn validate_recurrence(config: &RecurrenceConfig) -> Result<(), DataError> {
    if !(1..=999).contains(&config.interval) {
        return Err(DataError("重复间隔需要在 1 到 999 之间".into()));
    }
    if !matches!(config.unit.as_str(), "day" | "week" | "month" | "year") {
        return Err(DataError("重复单位无效".into()));
    }
    if !matches!(config.action.as_str(), "update_due" | "create_new") {
        return Err(DataError("重复方式无效".into()));
    }
    Ok(())
}

fn next_recurrence_due(due: i64, config: &RecurrenceConfig) -> Result<i64, DataError> {
    let date = Utc
        .timestamp_millis_opt(due)
        .single()
        .ok_or_else(|| DataError("截止时间无效".into()))?;
    let next = match config.unit.as_str() {
        "day" => date.checked_add_signed(chrono::Duration::days(config.interval as i64)),
        "week" => date.checked_add_signed(chrono::Duration::weeks(config.interval as i64)),
        "month" => date.checked_add_months(Months::new(config.interval as u32)),
        "year" => date.checked_add_months(Months::new(config.interval as u32 * 12)),
        _ => None,
    }
    .ok_or_else(|| DataError("无法计算下一次截止时间".into()))?;
    Ok(next.timestamp_millis())
}

fn recurrence_title(base: &str, due: i64, config: &RecurrenceConfig) -> String {
    let date = Utc.timestamp_millis_opt(due).single();
    match (config.unit.as_str(), date) {
        ("month", Some(date)) => format!("{}-{}月", base, date.month()),
        ("year", Some(date)) => format!("{}-{}年", base, date.year()),
        ("week", Some(date)) => format!("{}-{}月{}日", base, date.month(), date.day()),
        _ => base.to_string(),
    }
}

fn initialize_database(connection: &mut Connection) -> Result<(), DataError> {
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = FULL;
         PRAGMA trusted_schema = OFF;
         PRAGMA secure_delete = ON;",
    )?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at_utc_ms INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS app_metadata (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS device_settings (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL, updated_at_utc_ms INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS palette_colors (
             id TEXT PRIMARY KEY NOT NULL, row_index INTEGER NOT NULL CHECK(row_index BETWEEN 0 AND 2),
             column_index INTEGER NOT NULL CHECK(column_index BETWEEN 0 AND 7), value TEXT NOT NULL,
             created_at_utc_ms INTEGER NOT NULL, updated_at_utc_ms INTEGER NOT NULL, deleted_at_utc_ms INTEGER);
         CREATE TABLE IF NOT EXISTS categories (
             id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL COLLATE NOCASE, color_id TEXT NOT NULL REFERENCES palette_colors(id),
             created_at_utc_ms INTEGER NOT NULL, updated_at_utc_ms INTEGER NOT NULL, revision INTEGER NOT NULL DEFAULT 1,
             updated_by_device_id TEXT NOT NULL, deleted_at_utc_ms INTEGER, sort_order INTEGER NOT NULL DEFAULT 0,
             default_key TEXT, name_override TEXT, UNIQUE(name));
         CREATE TABLE IF NOT EXISTS tasks (
             id TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL, note TEXT NOT NULL DEFAULT '', category_id TEXT NOT NULL REFERENCES categories(id),
             status TEXT NOT NULL CHECK(status IN ('todo', 'completed')), due_at_utc_ms INTEGER, recurrence_json TEXT,
             completed_at_utc_ms INTEGER, created_at_utc_ms INTEGER NOT NULL, updated_at_utc_ms INTEGER NOT NULL,
             revision INTEGER NOT NULL DEFAULT 1, updated_by_device_id TEXT NOT NULL, deleted_at_utc_ms INTEGER);
         CREATE TABLE IF NOT EXISTS import_snapshots (
             id TEXT PRIMARY KEY NOT NULL, created_at_utc_ms INTEGER NOT NULL,
             source_file_name TEXT NOT NULL, snapshot_path TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS mcp_request_log (
             request_id TEXT NOT NULL, operation TEXT NOT NULL, response_json TEXT NOT NULL,
             created_at_utc_ms INTEGER NOT NULL, PRIMARY KEY(request_id, operation));",
    )?;
    ensure_category_sort_order(&transaction)?;
    let now = utc_now_ms();
    transaction.execute(
        "INSERT OR IGNORE INTO app_metadata (key, value) VALUES ('schema_version', '1')",
        [],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO app_metadata (key, value) VALUES ('device_id', ?1)",
        params![Uuid::new_v4().to_string()],
    )?;
    transaction.execute("INSERT OR IGNORE INTO device_settings (key, value, updated_at_utc_ms) VALUES (?1, 'light', ?2)", params![THEME_KEY, now])?;
    transaction.execute("INSERT OR IGNORE INTO device_settings (key, value, updated_at_utc_ms) VALUES (?1, 'true', ?2)", params![STARTUP_ENABLED_KEY, now])?;
    transaction.execute("INSERT OR IGNORE INTO device_settings (key, value, updated_at_utc_ms) VALUES (?1, 'true', ?2)", params![MCP_ENABLED_KEY, now])?;
    transaction.execute(
        "INSERT OR IGNORE INTO device_settings (key, value, updated_at_utc_ms) VALUES (?1, ?2, ?3)",
        params![LOCALE_KEY, DEFAULT_LOCALE, now],
    )?;
    for (row, values) in PALETTE.iter().enumerate() {
        for (column, value) in values.iter().enumerate() {
            transaction.execute(
                "INSERT OR IGNORE INTO palette_colors (id, row_index, column_index, value, created_at_utc_ms, updated_at_utc_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![palette_id(row, column), row as u8, column as u8, value, now],
            )?;
        }
    }
    let device_id: String = transaction.query_row(
        "SELECT value FROM app_metadata WHERE key = 'device_id'",
        [],
        |row| row.get(0),
    )?;
    let category_count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM categories WHERE deleted_at_utc_ms IS NULL",
        [],
        |row| row.get(0),
    )?;
    if category_count == 0 {
        for (sort_order, (key, name, color_column)) in DEFAULT_CATEGORIES.iter().enumerate() {
            transaction.execute(
                "INSERT INTO categories (id, name, default_key, name_override, color_id, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id, sort_order) VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?5, 1, ?6, ?7)",
                params![Uuid::new_v4().to_string(), name, key, palette_id(1, *color_column), now, device_id, sort_order as i64],
            )?;
        }
    }
    migrate_default_categories_v3(&transaction, now, &device_id)?;
    migrate_category_hard_delete_v4(&transaction, now, &device_id)?;
    migrate_task_hard_delete_v5(&transaction, now)?;
    migrate_localized_default_categories_v6(&transaction, now, &device_id)?;
    migrate_other_default_color_v7(&transaction, now, &device_id)?;
    migrate_task_recurrence_v8(&transaction, now)?;
    transaction.execute(
        "INSERT OR IGNORE INTO schema_migrations (version, applied_at_utc_ms) VALUES (1, ?1)",
        params![now],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO schema_migrations (version, applied_at_utc_ms) VALUES (2, ?1)",
        params![now],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO schema_migrations (version, applied_at_utc_ms) VALUES (3, ?1)",
        params![now],
    )?;
    transaction.commit()?;
    Ok(())
}

fn migrate_task_hard_delete_v5(
    transaction: &rusqlite::Transaction<'_>,
    now: i64,
) -> Result<(), DataError> {
    let already_applied = transaction
        .query_row(
            "SELECT 1 FROM schema_migrations WHERE version = 5",
            [],
            |_| Ok(()),
        )
        .optional()?;
    if already_applied.is_none() {
        transaction.execute("DELETE FROM tasks WHERE deleted_at_utc_ms IS NOT NULL", [])?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, applied_at_utc_ms) VALUES (5, ?1)",
            params![now],
        )?;
    }
    Ok(())
}

/// Adds locale-safe identity columns and maps the original Chinese seed labels.
/// This is intentionally additive: custom categories and task UUID links stay intact.
fn migrate_localized_default_categories_v6(
    transaction: &rusqlite::Transaction<'_>,
    now: i64,
    device_id: &str,
) -> Result<(), DataError> {
    let applied = transaction
        .query_row(
            "SELECT 1 FROM schema_migrations WHERE version = 6",
            [],
            |_| Ok(()),
        )
        .optional()?;
    if applied.is_some() {
        return Ok(());
    }

    let columns = transaction
        .prepare("PRAGMA table_info(categories)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|name| name == "default_key") {
        transaction.execute_batch("ALTER TABLE categories ADD COLUMN default_key TEXT;")?;
    }
    if !columns.iter().any(|name| name == "name_override") {
        transaction.execute_batch("ALTER TABLE categories ADD COLUMN name_override TEXT;")?;
    }

    for (sort_order, (key, canonical_name, _)) in DEFAULT_CATEGORIES.iter().enumerate() {
        transaction.execute(
            "UPDATE categories SET default_key = ?1, sort_order = ?2,
             updated_at_utc_ms = ?3, revision = revision + 1, updated_by_device_id = ?4
             WHERE name = ?5 AND default_key IS NULL AND deleted_at_utc_ms IS NULL",
            params![key, sort_order as i64, now, device_id, canonical_name],
        )?;
    }
    transaction.execute(
        "INSERT INTO schema_migrations (version, applied_at_utc_ms) VALUES (6, ?1)",
        params![now],
    )?;
    Ok(())
}

/// Updates only the former untouched default color for “Other”. A manually
/// chosen color or a renamed default category is deliberately left unchanged.
fn migrate_other_default_color_v7(
    transaction: &rusqlite::Transaction<'_>,
    now: i64,
    device_id: &str,
) -> Result<(), DataError> {
    let applied = transaction
        .query_row(
            "SELECT 1 FROM schema_migrations WHERE version = 7",
            [],
            |_| Ok(()),
        )
        .optional()?;
    if applied.is_some() {
        return Ok(());
    }
    transaction.execute(
        "UPDATE categories SET color_id = ?1, updated_at_utc_ms = ?2,
         revision = revision + 1, updated_by_device_id = ?3
         WHERE default_key = 'other' AND name_override IS NULL AND color_id = ?4
         AND deleted_at_utc_ms IS NULL",
        params![palette_id(1, 7), now, device_id, palette_id(1, 3)],
    )?;
    transaction.execute(
        "INSERT INTO schema_migrations (version, applied_at_utc_ms) VALUES (7, ?1)",
        params![now],
    )?;
    Ok(())
}

fn migrate_task_recurrence_v8(
    transaction: &rusqlite::Transaction<'_>,
    now: i64,
) -> Result<(), DataError> {
    if transaction
        .query_row(
            "SELECT 1 FROM schema_migrations WHERE version = 8",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Ok(());
    }
    let columns = transaction
        .prepare("PRAGMA table_info(tasks)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|name| name == "recurrence_json") {
        transaction.execute_batch("ALTER TABLE tasks ADD COLUMN recurrence_json TEXT;")?;
    }
    transaction.execute(
        "INSERT INTO schema_migrations (version, applied_at_utc_ms) VALUES (8, ?1)",
        params![now],
    )?;
    Ok(())
}

fn read_bootstrap(connection: &Connection) -> Result<BootstrapDto, DataError> {
    let device_id = connection.query_row(
        "SELECT value FROM app_metadata WHERE key = 'device_id'",
        [],
        |row| row.get(0),
    )?;
    let theme = connection
        .query_row(
            "SELECT value FROM device_settings WHERE key = ?1",
            params![THEME_KEY],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_else(|| "light".to_string());
    let startup_enabled = read_startup_enabled(connection)?;
    let mcp_enabled = read_mcp_enabled(connection)?;
    let interface_transparency = read_interface_transparency(connection)?;
    let locale = read_locale(connection)?;
    let mut palette_statement = connection.prepare("SELECT id, row_index, column_index, value FROM palette_colors WHERE deleted_at_utc_ms IS NULL ORDER BY row_index, column_index")?;
    let palette = palette_statement
        .query_map([], |row| {
            Ok(PaletteColorDto {
                id: row.get(0)?,
                row: row.get(1)?,
                column: row.get(2)?,
                value: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut category_statement = connection.prepare(
        "SELECT categories.id, categories.name, categories.default_key, categories.name_override, categories.color_id, palette_colors.value, categories.revision FROM categories JOIN palette_colors ON palette_colors.id = categories.color_id WHERE categories.deleted_at_utc_ms IS NULL ORDER BY categories.sort_order, categories.created_at_utc_ms, categories.id",
    )?;
    let categories = category_statement
        .query_map([], |row| {
            Ok(CategoryDto {
                id: row.get(0)?,
                name: row.get(1)?,
                default_key: row.get(2)?,
                name_override: row.get(3)?,
                color_id: row.get(4)?,
                color: row.get(5)?,
                revision: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BootstrapDto {
        device_id,
        theme,
        locale,
        startup_enabled,
        mcp_enabled,
        interface_transparency,
        categories,
        palette,
    })
}

fn read_startup_enabled(connection: &Connection) -> Result<bool, DataError> {
    let value = connection
        .query_row(
            "SELECT value FROM device_settings WHERE key = ?1",
            params![STARTUP_ENABLED_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_else(|| "true".to_string());
    Ok(value == "true")
}

fn read_interface_transparency(connection: &Connection) -> Result<u8, DataError> {
    let value = connection
        .query_row(
            "SELECT value FROM device_settings WHERE key = ?1",
            params![INTERFACE_TRANSPARENCY_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(value
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|value| *value % 5 == 0)
        .map(|value| value.min(30))
        .unwrap_or(5))
}

fn read_mcp_enabled(connection: &Connection) -> Result<bool, DataError> {
    let value = connection
        .query_row(
            "SELECT value FROM device_settings WHERE key = ?1",
            params![MCP_ENABLED_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_else(|| "true".to_string());
    Ok(value == "true")
}

fn read_locale(connection: &Connection) -> Result<String, DataError> {
    Ok(connection
        .query_row(
            "SELECT value FROM device_settings WHERE key = ?1",
            params![LOCALE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .filter(|value| {
            matches!(
                value.as_str(),
                "zh-CN" | "en" | "de" | "fr" | "it" | "es" | "ja" | "zh-TW"
            )
        })
        .unwrap_or_else(|| DEFAULT_LOCALE.to_string()))
}

fn palette_id(row: usize, column: usize) -> String {
    format!("00000000-0000-4000-8000-{:012}", row * 8 + column + 1)
}
fn utc_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

fn ensure_category_sort_order(transaction: &rusqlite::Transaction<'_>) -> Result<(), DataError> {
    let mut statement = transaction.prepare("PRAGMA table_info(categories)")?;
    let has_sort_order = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|name| name == "sort_order");
    if !has_sort_order {
        transaction.execute_batch(
            "ALTER TABLE categories ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    Ok(())
}

fn migrate_category_hard_delete_v4(
    transaction: &rusqlite::Transaction<'_>,
    now: i64,
    device_id: &str,
) -> Result<(), DataError> {
    let already_applied = transaction
        .query_row(
            "SELECT 1 FROM schema_migrations WHERE version = 4",
            [],
            |_| Ok(()),
        )
        .optional()?;
    if already_applied.is_some() {
        return Ok(());
    }

    let fallback_category_id: String = transaction.query_row(
        "SELECT id FROM categories WHERE deleted_at_utc_ms IS NULL ORDER BY sort_order, id LIMIT 1",
        [],
        |row| row.get(0),
    )?;
    transaction.execute(
        "UPDATE tasks SET category_id = ?1, updated_at_utc_ms = ?2, revision = revision + 1,
         updated_by_device_id = ?3 WHERE category_id IN (SELECT id FROM categories WHERE deleted_at_utc_ms IS NOT NULL)",
        params![fallback_category_id, now, device_id],
    )?;
    transaction.execute(
        "DELETE FROM categories WHERE deleted_at_utc_ms IS NOT NULL",
        [],
    )?;
    transaction.execute(
        "INSERT INTO schema_migrations (version, applied_at_utc_ms) VALUES (4, ?1)",
        params![now],
    )?;
    Ok(())
}

fn migrate_default_categories_v3(
    transaction: &rusqlite::Transaction<'_>,
    now: i64,
    device_id: &str,
) -> Result<(), DataError> {
    let already_applied = transaction
        .query_row(
            "SELECT 1 FROM schema_migrations WHERE version = 3",
            [],
            |_| Ok(()),
        )
        .optional()?;
    if already_applied.is_some() {
        return Ok(());
    }

    let legacy_default_count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM categories WHERE deleted_at_utc_ms IS NULL
         AND name IN ('个人', '团队', '工作', '生活', '出行', '财务', '学习')",
        [],
        |row| row.get(0),
    )?;
    if legacy_default_count != 7 {
        return Ok(());
    }

    let other_exists = transaction
        .query_row(
            "SELECT 1 FROM categories WHERE name = '其他' AND deleted_at_utc_ms IS NULL",
            [],
            |_| Ok(()),
        )
        .optional()?;
    if other_exists.is_none() {
        transaction.execute(
            "INSERT INTO categories (id, name, color_id, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id, sort_order)
             VALUES (?1, '其他', ?2, ?3, ?3, 1, ?4, 7)",
            params![Uuid::new_v4().to_string(), palette_id(1, 3), now, device_id],
        )?;
    }

    for (sort_order, (_, name, _)) in DEFAULT_CATEGORIES.iter().enumerate() {
        transaction.execute(
            "UPDATE categories SET sort_order = ?1, updated_at_utc_ms = ?2, revision = revision + 1,
             updated_by_device_id = ?3 WHERE name = ?4 AND deleted_at_utc_ms IS NULL",
            params![sort_order as i64, now, device_id, name],
        )?;
    }
    let mut extras = transaction.prepare(
        "SELECT id FROM categories WHERE deleted_at_utc_ms IS NULL
         AND name NOT IN ('个人', '团队', '工作', '生活', '出行', '财务', '学习', '其他')
         ORDER BY sort_order, created_at_utc_ms, id",
    )?;
    let extra_ids = extras
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for (offset, id) in extra_ids.iter().enumerate() {
        transaction.execute(
            "UPDATE categories SET sort_order = ?1, updated_at_utc_ms = ?2, revision = revision + 1,
             updated_by_device_id = ?3 WHERE id = ?4",
            params![(DEFAULT_CATEGORIES.len() + offset) as i64, now, device_id, id],
        )?;
    }
    Ok(())
}

fn validate_task_input(title: &str, note: &str) -> Result<String, DataError> {
    let title = title.trim();
    let title_length = title.chars().count();
    if !(1..=200).contains(&title_length) {
        return Err(DataError("标题需为 1–200 个字符".into()));
    }
    if note.chars().count() > 2_000 {
        return Err(DataError("备注最多 2,000 个字符".into()));
    }
    Ok(title.to_string())
}

fn validate_due_at(due_at_utc_ms: Option<i64>) -> Result<(), DataError> {
    if let Some(value) = due_at_utc_ms {
        const EARLIEST_SUPPORTED_UTC_MS: i64 = 946_684_800_000; // 2000-01-01T00:00:00Z
        const LATEST_SUPPORTED_UTC_MS: i64 = 4_102_444_800_000; // 2100-01-01T00:00:00Z
        if !(EARLIEST_SUPPORTED_UTC_MS..=LATEST_SUPPORTED_UTC_MS).contains(&value) {
            return Err(DataError("截止时间超出支持范围".into()));
        }
    }
    Ok(())
}

fn validate_category_name(name: &str) -> Result<String, DataError> {
    let name = name.trim();
    let length = name.chars().count();
    if !(1..=30).contains(&length) {
        return Err(DataError("分类名称需为 1–30 个字符".into()));
    }
    Ok(name.to_string())
}

fn validate_default_category_key(key: &str) -> Result<(), DataError> {
    if DEFAULT_CATEGORIES.iter().any(|(known, _, _)| *known == key) {
        Ok(())
    } else {
        Err(DataError("导入分类的默认标识无效".into()))
    }
}

fn validate_status(status: &str) -> Result<(), DataError> {
    if matches!(status, "todo" | "completed") {
        Ok(())
    } else {
        Err(DataError("不支持的事项状态".into()))
    }
}

fn read_device_id(connection: &rusqlite::Transaction<'_>) -> Result<String, DataError> {
    connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key = 'device_id'",
            [],
            |row| row.get(0),
        )
        .map_err(Into::into)
}

fn ensure_active_category(
    connection: &rusqlite::Transaction<'_>,
    category_id: &str,
) -> Result<(), DataError> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM categories WHERE id = ?1 AND deleted_at_utc_ms IS NULL",
            params![category_id],
            |_| Ok(()),
        )
        .optional()?;
    if exists.is_none() {
        return Err(DataError("请选择有效分类".into()));
    }
    Ok(())
}

fn ensure_palette_color(
    connection: &rusqlite::Transaction<'_>,
    color_id: &str,
) -> Result<(), DataError> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM palette_colors WHERE id = ?1 AND deleted_at_utc_ms IS NULL",
            params![color_id],
            |_| Ok(()),
        )
        .optional()?;
    if exists.is_none() {
        return Err(DataError("请选择有效颜色".into()));
    }
    Ok(())
}

fn ensure_category_name_available(
    connection: &rusqlite::Transaction<'_>,
    name: &str,
    exclude_id: Option<&str>,
) -> Result<(), DataError> {
    let existing = connection
        .query_row(
            "SELECT id FROM categories WHERE name = ?1",
            params![name],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if existing.as_deref().is_some_and(|id| Some(id) != exclude_id) {
        return Err(DataError("已存在同名分类".into()));
    }
    Ok(())
}

fn next_available_category_name(
    connection: &rusqlite::Transaction<'_>,
    base_name: &str,
) -> Result<String, DataError> {
    for sequence in 0..10_000 {
        let candidate = if sequence == 0 {
            base_name.to_string()
        } else {
            format!("{base_name}-{sequence}")
        };
        validate_category_name(&candidate)?;
        let exists = connection
            .query_row(
                "SELECT 1 FROM categories WHERE name = ?1",
                params![candidate],
                |_| Ok(()),
            )
            .optional()?;
        if exists.is_none() {
            return Ok(candidate);
        }
    }
    Err(DataError("无法自动生成可用分类名称".into()))
}

fn next_available_color_id(connection: &rusqlite::Transaction<'_>) -> Result<String, DataError> {
    let available = connection
        .query_row(
            "SELECT palette_colors.id FROM palette_colors
             WHERE palette_colors.deleted_at_utc_ms IS NULL
             AND NOT EXISTS (SELECT 1 FROM categories WHERE categories.color_id = palette_colors.id AND categories.deleted_at_utc_ms IS NULL)
             ORDER BY palette_colors.row_index, palette_colors.column_index LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(color_id) = available {
        return Ok(color_id);
    }
    connection
        .query_row(
            "SELECT id FROM palette_colors WHERE deleted_at_utc_ms IS NULL ORDER BY row_index, column_index LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| DataError("调色板不可用".into()))
}

fn read_category(connection: &Connection, id: &str) -> Result<CategoryDto, DataError> {
    connection
        .query_row(
            "SELECT categories.id, categories.name, categories.default_key, categories.name_override, categories.color_id, palette_colors.value, categories.revision FROM categories
             JOIN palette_colors ON palette_colors.id = categories.color_id
             WHERE categories.id = ?1 AND categories.deleted_at_utc_ms IS NULL",
            params![id],
            |row| {
                Ok(CategoryDto {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    default_key: row.get(2)?,
                    name_override: row.get(3)?,
                    color_id: row.get(4)?,
                    color: row.get(5)?,
                    revision: row.get(6)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| DataError("分类不存在或已删除".into()))
}

fn read_category_transaction(
    transaction: &rusqlite::Transaction<'_>,
    id: &str,
) -> Result<CategoryDto, DataError> {
    transaction
        .query_row(
            "SELECT categories.id, categories.name, categories.default_key, categories.name_override, categories.color_id, palette_colors.value, categories.revision
             FROM categories JOIN palette_colors ON palette_colors.id = categories.color_id
             WHERE categories.id = ?1 AND categories.deleted_at_utc_ms IS NULL",
            params![id],
            |row| {
                Ok(CategoryDto {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    default_key: row.get(2)?,
                    name_override: row.get(3)?,
                    color_id: row.get(4)?,
                    color: row.get(5)?,
                    revision: row.get(6)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| DataError("分类不存在或已删除".into()))
}

fn read_task(connection: &Connection, id: &str) -> Result<TaskDto, DataError> {
    connection.query_row(
        "SELECT tasks.id, tasks.title, tasks.note, tasks.category_id, categories.name, categories.default_key, categories.name_override, palette_colors.value, tasks.status,
         tasks.due_at_utc_ms, tasks.recurrence_json, tasks.created_at_utc_ms, tasks.updated_at_utc_ms, tasks.completed_at_utc_ms, tasks.revision FROM tasks
         JOIN categories ON categories.id = tasks.category_id JOIN palette_colors ON palette_colors.id = categories.color_id
         WHERE tasks.id = ?1 AND tasks.deleted_at_utc_ms IS NULL",
        params![id], task_from_row,
    ).optional()?.ok_or_else(|| DataError("事项不存在或已删除".into()))
}

fn read_task_transaction(
    transaction: &rusqlite::Transaction<'_>,
    id: &str,
) -> Result<TaskDto, DataError> {
    transaction.query_row(
        "SELECT tasks.id, tasks.title, tasks.note, tasks.category_id, categories.name, categories.default_key, categories.name_override, palette_colors.value, tasks.status, tasks.due_at_utc_ms, tasks.recurrence_json, tasks.created_at_utc_ms, tasks.updated_at_utc_ms, tasks.completed_at_utc_ms, tasks.revision FROM tasks JOIN categories ON categories.id = tasks.category_id JOIN palette_colors ON palette_colors.id = categories.color_id WHERE tasks.id = ?1 AND tasks.deleted_at_utc_ms IS NULL",
        params![id],
        |row| task_from_row(row),
    ).optional()?.ok_or_else(|| DataError("事项不存在或已删除".into()))
}

fn read_mcp_request_task(
    transaction: &rusqlite::Transaction<'_>,
    request_id: &str,
    operation: &str,
) -> Result<Option<TaskDto>, DataError> {
    let encoded: Option<String> = transaction
        .query_row(
            "SELECT response_json FROM mcp_request_log WHERE request_id = ?1 AND operation = ?2",
            params![request_id, operation],
            |row| row.get(0),
        )
        .optional()?;
    encoded
        .map(|value| {
            serde_json::from_str(&value).map_err(|_| DataError("MCP 请求记录无法读取".into()))
        })
        .transpose()
}

fn save_mcp_request_task(
    transaction: &rusqlite::Transaction<'_>,
    request_id: &str,
    operation: &str,
    task: &TaskDto,
) -> Result<(), DataError> {
    let response_json =
        serde_json::to_string(task).map_err(|_| DataError("MCP 请求结果无法保存".into()))?;
    transaction.execute(
        "INSERT INTO mcp_request_log (request_id, operation, response_json, created_at_utc_ms) VALUES (?1, ?2, ?3, ?4)",
        params![request_id, operation, response_json, utc_now_ms()],
    )?;
    Ok(())
}

fn read_mcp_request_category(
    transaction: &rusqlite::Transaction<'_>,
    request_id: &str,
    operation: &str,
) -> Result<Option<CategoryDto>, DataError> {
    let encoded: Option<String> = transaction
        .query_row(
            "SELECT response_json FROM mcp_request_log WHERE request_id = ?1 AND operation = ?2",
            params![request_id, operation],
            |row| row.get(0),
        )
        .optional()?;
    encoded
        .map(|value| {
            serde_json::from_str(&value).map_err(|_| DataError("MCP 请求记录无法读取".into()))
        })
        .transpose()
}

fn save_mcp_request_category(
    transaction: &rusqlite::Transaction<'_>,
    request_id: &str,
    operation: &str,
    category: &CategoryDto,
) -> Result<(), DataError> {
    let response_json =
        serde_json::to_string(category).map_err(|_| DataError("MCP 请求结果无法保存".into()))?;
    transaction.execute(
        "INSERT INTO mcp_request_log (request_id, operation, response_json, created_at_utc_ms) VALUES (?1, ?2, ?3, ?4)",
        params![request_id, operation, response_json, utc_now_ms()],
    )?;
    Ok(())
}

fn read_mcp_delete_result(
    transaction: &rusqlite::Transaction<'_>,
    request_id: &str,
    operation: &str,
) -> Result<Option<McpDeleteResultDto>, DataError> {
    let encoded: Option<String> = transaction
        .query_row(
            "SELECT response_json FROM mcp_request_log WHERE request_id = ?1 AND operation = ?2",
            params![request_id, operation],
            |row| row.get(0),
        )
        .optional()?;
    encoded
        .map(|value| {
            serde_json::from_str(&value).map_err(|_| DataError("MCP 请求记录无法读取".into()))
        })
        .transpose()
}

fn save_mcp_delete_result(
    transaction: &rusqlite::Transaction<'_>,
    request_id: &str,
    operation: &str,
    result: &McpDeleteResultDto,
) -> Result<(), DataError> {
    let response_json =
        serde_json::to_string(result).map_err(|_| DataError("MCP 请求结果无法保存".into()))?;
    transaction.execute("INSERT INTO mcp_request_log (request_id, operation, response_json, created_at_utc_ms) VALUES (?1, ?2, ?3, ?4)", params![request_id, operation, response_json, utc_now_ms()])?;
    Ok(())
}

fn read_tasks(connection: &Connection, status: &str) -> Result<Vec<TaskDto>, DataError> {
    let ordering = if status == "todo" {
        "CASE WHEN due_at_utc_ms IS NOT NULL AND due_at_utc_ms < ?2 THEN 0 ELSE 1 END, CASE WHEN due_at_utc_ms IS NULL THEN 1 ELSE 0 END, due_at_utc_ms ASC, tasks.updated_at_utc_ms DESC, tasks.id"
    } else {
        "tasks.completed_at_utc_ms DESC, tasks.updated_at_utc_ms DESC, tasks.id"
    };
    let query = format!(
        "SELECT tasks.id, tasks.title, tasks.note, tasks.category_id, categories.name, categories.default_key, categories.name_override, palette_colors.value, tasks.status,
         tasks.due_at_utc_ms, tasks.recurrence_json, tasks.created_at_utc_ms, tasks.updated_at_utc_ms, tasks.completed_at_utc_ms, tasks.revision FROM tasks
         JOIN categories ON categories.id = tasks.category_id JOIN palette_colors ON palette_colors.id = categories.color_id
         WHERE tasks.status = ?1 AND tasks.deleted_at_utc_ms IS NULL ORDER BY {ordering}"
    );
    let mut statement = connection.prepare(&query)?;
    let rows = if status == "todo" {
        statement.query_map(params![status, utc_now_ms()], task_from_row)?
    } else {
        statement.query_map(params![status], task_from_row)?
    };
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn task_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskDto> {
    Ok(TaskDto {
        id: row.get(0)?,
        title: row.get(1)?,
        note: row.get(2)?,
        category_id: row.get(3)?,
        category_name: row.get(4)?,
        category_default_key: row.get(5)?,
        category_name_override: row.get(6)?,
        category_color: row.get(7)?,
        status: row.get(8)?,
        due_at_utc_ms: row.get(9)?,
        recurrence_json: row.get(10)?,
        created_at_utc_ms: row.get(11)?,
        updated_at_utc_ms: row.get(12)?,
        completed_at_utc_ms: row.get(13)?,
        revision: row.get(14)?,
    })
}

fn validate_import_package(package: &PlaintextExportDto) -> Result<(), DataError> {
    if package.schema_version != 1 {
        return Err(DataError("导入文件版本不受支持".into()));
    }
    validate_uuid(&package.export_id, "导出标识")?;
    validate_uuid(&package.source_device_id, "来源设备标识")?;
    if package.palette.len() != 24 {
        return Err(DataError("导入文件必须包含完整的 24 色调色板".into()));
    }
    let mut palette_locations = std::collections::HashSet::new();
    let mut palette_ids = std::collections::HashSet::new();
    for color in &package.palette {
        validate_uuid(&color.id, "调色板标识")?;
        if color.row > 2 || color.column > 7 || !palette_locations.insert((color.row, color.column))
        {
            return Err(DataError("导入调色板不完整或存在重复颜色位置".into()));
        }
        if !palette_ids.insert(color.id.as_str()) || !is_hex_color(&color.value) {
            return Err(DataError("导入调色板颜色无效".into()));
        }
    }
    let mut category_ids = std::collections::HashSet::new();
    let mut category_names = std::collections::HashSet::new();
    let mut default_category_keys = std::collections::HashSet::new();
    for category in &package.categories {
        validate_uuid(&category.id, "分类标识")?;
        let name = validate_category_name(&category.name)?;
        if let Some(key) = &category.default_key {
            validate_default_category_key(key)?;
            if !default_category_keys.insert(key.as_str()) {
                return Err(DataError("导入文件存在重复默认分类标识".into()));
            }
        }
        if let Some(name_override) = &category.name_override {
            validate_category_name(name_override)?;
        }
        if !category_ids.insert(category.id.as_str()) || !category_names.insert(name.to_lowercase())
        {
            return Err(DataError("导入文件存在重复分类".into()));
        }
        if !palette_ids.contains(category.color_id.as_str()) {
            return Err(DataError("导入分类引用了不存在的颜色".into()));
        }
    }
    let mut task_ids = std::collections::HashSet::new();
    for task in &package.tasks {
        validate_uuid(&task.id, "事项标识")?;
        validate_task_input(&task.title, &task.note)?;
        validate_due_at(task.due_at_utc_ms)?;
        if !task_ids.insert(task.id.as_str()) || !category_ids.contains(task.category_id.as_str()) {
            return Err(DataError("导入事项的标识或分类引用无效".into()));
        }
        if !matches!(task.status.as_str(), "todo" | "completed") {
            return Err(DataError("导入事项状态无效".into()));
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct LocalImportCategory {
    id: String,
    name: String,
    default_key: Option<String>,
    name_override: Option<String>,
    color_id: String,
    updated_at_utc_ms: i64,
    revision: i64,
    updated_by_device_id: String,
}

fn read_local_import_categories(
    connection: &Connection,
) -> Result<Vec<LocalImportCategory>, DataError> {
    let mut statement = connection.prepare(
        "SELECT id, name, default_key, name_override, color_id, updated_at_utc_ms, revision, updated_by_device_id
         FROM categories WHERE deleted_at_utc_ms IS NULL",
    )?;
    let categories = statement
        .query_map([], |row| {
            Ok(LocalImportCategory {
                id: row.get(0)?,
                name: row.get(1)?,
                default_key: row.get(2)?,
                name_override: row.get(3)?,
                color_id: row.get(4)?,
                updated_at_utc_ms: row.get(5)?,
                revision: row.get(6)?,
                updated_by_device_id: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(categories)
}

/// Default categories are identity-matched by stable key. UUID is primary for
/// normal categories. The canonical stored name is only a legacy fallback.
fn find_matching_category<'a>(
    local_categories: &'a [LocalImportCategory],
    incoming: &ExportCategoryDto,
    incoming_name: &str,
) -> Option<&'a LocalImportCategory> {
    if let Some(default_key) = incoming.default_key.as_deref() {
        return local_categories
            .iter()
            .find(|category| category.default_key.as_deref() == Some(default_key))
            .or_else(|| {
                local_categories
                    .iter()
                    .find(|category| category.id == incoming.id)
            })
            .or_else(|| {
                local_categories.iter().find(|category| {
                    category.default_key.is_none()
                        && category.name.eq_ignore_ascii_case(incoming_name)
                })
            });
    }

    local_categories
        .iter()
        .find(|category| category.id == incoming.id)
        .or_else(|| {
            local_categories.iter().find(|category| {
                category.default_key.is_none() && category.name.eq_ignore_ascii_case(incoming_name)
            })
        })
}

fn incoming_category_metadata_wins(
    incoming: &ExportCategoryDto,
    local: &LocalImportCategory,
) -> bool {
    use std::cmp::Ordering;
    match incoming.updated_at_utc_ms.cmp(&local.updated_at_utc_ms) {
        Ordering::Greater => return true,
        Ordering::Less => return false,
        Ordering::Equal => {}
    }
    match incoming.revision.cmp(&local.revision) {
        Ordering::Greater => return true,
        Ordering::Less => return false,
        Ordering::Equal => {}
    }
    if incoming.name_override == local.name_override && incoming.default_key == local.default_key {
        return false;
    }
    incoming.updated_by_device_id > local.updated_by_device_id
}

fn build_import_preview(
    connection: &Connection,
    package: &PlaintextExportDto,
    source_file_name: &str,
) -> Result<ImportPreviewDto, DataError> {
    let mut local_palette = std::collections::HashMap::new();
    let mut palette_statement = connection.prepare(
        "SELECT id, row_index, column_index FROM palette_colors WHERE deleted_at_utc_ms IS NULL",
    )?;
    for row in palette_statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, u8>(1)?,
            row.get::<_, u8>(2)?,
        ))
    })? {
        let (id, row_index, column_index) = row?;
        local_palette.insert((row_index, column_index), id);
    }
    let imported_color_ids = package
        .palette
        .iter()
        .map(|color| {
            let local_id = local_palette
                .get(&(color.row, color.column))
                .ok_or_else(|| DataError("本机调色板不可用".into()))?;
            Ok((color.id.as_str(), local_id.as_str()))
        })
        .collect::<Result<std::collections::HashMap<_, _>, DataError>>()?;

    let local_categories = read_local_import_categories(connection)?;

    let mut category_ids = std::collections::HashMap::new();
    let mut new_categories = 0;
    let mut updated_categories = 0;
    let mut kept_categories = 0;
    for category in &package.categories {
        let imported_color_id = imported_color_ids
            .get(category.color_id.as_str())
            .ok_or_else(|| DataError("导入分类颜色无效".into()))?;
        let name = validate_category_name(&category.name)?;
        if let Some(local) = find_matching_category(&local_categories, category, &name) {
            category_ids.insert(category.id.as_str(), local.id.clone());
            let metadata_changes = category.default_key.is_some()
                && (local.default_key != category.default_key
                    || (incoming_category_metadata_wins(category, local)
                        && local.name_override != category.name_override));
            if local.color_id == *imported_color_id && !metadata_changes {
                kept_categories += 1;
            } else {
                updated_categories += 1;
            }
        } else {
            category_ids.insert(category.id.as_str(), category.id.clone());
            new_categories += 1;
        }
    }

    let mut new_tasks = 0;
    let mut updated_tasks = 0;
    let mut kept_tasks = 0;
    for task in &package.tasks {
        let category_id = category_ids
            .get(task.category_id.as_str())
            .ok_or_else(|| DataError("导入事项分类无效".into()))?;
        let local = connection.query_row(
            "SELECT title, note, category_id, status, due_at_utc_ms, completed_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id
             FROM tasks WHERE id = ?1 AND deleted_at_utc_ms IS NULL",
            params![task.id],
            |row| Ok(LocalImportTask {
                title: row.get(0)?, note: row.get(1)?, category_id: row.get(2)?, status: row.get(3)?,
                due_at_utc_ms: row.get(4)?, completed_at_utc_ms: row.get(5)?, updated_at_utc_ms: row.get(6)?,
                revision: row.get(7)?, updated_by_device_id: row.get(8)?,
            }),
        ).optional()?;
        match local {
            None => new_tasks += 1,
            Some(local) if incoming_task_wins(task, category_id, &local) => updated_tasks += 1,
            Some(_) => kept_tasks += 1,
        }
    }
    Ok(ImportPreviewDto {
        session_id: String::new(),
        source_file_name: source_file_name.to_string(),
        source_device_id: package.source_device_id.clone(),
        exported_at_utc_ms: package.exported_at_utc_ms,
        task_count: package.tasks.len(),
        category_count: package.categories.len(),
        palette_count: package.palette.len(),
        new_tasks,
        updated_tasks,
        kept_tasks,
        new_categories,
        updated_categories,
        kept_categories,
    })
}

fn merge_plaintext_package(
    transaction: &rusqlite::Transaction<'_>,
    package: &PlaintextExportDto,
    source_file_name: &str,
) -> Result<ImportResultDto, DataError> {
    let mut local_palette = std::collections::HashMap::new();
    let mut palette_statement = transaction.prepare(
        "SELECT id, row_index, column_index FROM palette_colors WHERE deleted_at_utc_ms IS NULL",
    )?;
    for row in palette_statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, u8>(1)?,
            row.get::<_, u8>(2)?,
        ))
    })? {
        let (id, row_index, column_index) = row?;
        local_palette.insert((row_index, column_index), id);
    }
    let imported_colors = package
        .palette
        .iter()
        .map(|color| {
            let local_id = local_palette
                .get(&(color.row, color.column))
                .ok_or_else(|| DataError("本机调色板不可用".into()))?;
            Ok((color.id.as_str(), local_id.as_str()))
        })
        .collect::<Result<std::collections::HashMap<_, _>, DataError>>()?;

    let mut local_categories = read_local_import_categories(transaction)?;
    let mut imported_category_ids = std::collections::HashMap::new();
    let mut new_categories = 0;
    let mut updated_categories = 0;
    let mut kept_categories = 0;
    for category in &package.categories {
        let name = validate_category_name(&category.name)?;
        let color_id = imported_colors
            .get(category.color_id.as_str())
            .ok_or_else(|| DataError("导入分类颜色无效".into()))?;
        if let Some(local) = find_matching_category(&local_categories, category, &name).cloned() {
            imported_category_ids.insert(category.id.as_str(), local.id.clone());
            let metadata_wins =
                category.default_key.is_some() && incoming_category_metadata_wins(category, &local);
            let must_attach_default_key =
                category.default_key.is_some() && local.default_key != category.default_key;
            let color_changes = local.color_id != *color_id;
            let override_changes = metadata_wins && local.name_override != category.name_override;
            if !color_changes && !must_attach_default_key && !override_changes {
                kept_categories += 1;
            } else {
                transaction.execute(
                    "UPDATE categories SET
                       color_id = ?1,
                       default_key = CASE WHEN default_key IS NULL THEN ?2 ELSE default_key END,
                       name_override = CASE WHEN ?3 THEN ?4 ELSE name_override END,
                       updated_at_utc_ms = ?5, revision = revision + 1, updated_by_device_id = ?6
                     WHERE id = ?7",
                    params![
                        *color_id,
                        category.default_key.as_deref(),
                        metadata_wins,
                        category.name_override.as_deref(),
                        utc_now_ms(),
                        read_device_id(transaction)?,
                        local.id,
                    ],
                )?;
                updated_categories += 1;
            }
        } else {
            let id_conflict = transaction
                .query_row(
                    "SELECT 1 FROM categories WHERE id = ?1",
                    params![category.id],
                    |_| Ok(()),
                )
                .optional()?;
            if id_conflict.is_some() {
                return Err(DataError("导入分类标识与本机数据冲突".into()));
            }
            transaction.execute(
                "INSERT INTO categories (id, name, default_key, name_override, color_id, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![category.id, name, category.default_key, category.name_override, *color_id, category.created_at_utc_ms, category.updated_at_utc_ms, category.revision, category.updated_by_device_id, category.sort_order],
            )?;
            imported_category_ids.insert(category.id.as_str(), category.id.clone());
            local_categories.push(LocalImportCategory {
                id: category.id.clone(),
                name,
                default_key: category.default_key.clone(),
                name_override: category.name_override.clone(),
                color_id: (*color_id).to_string(),
                updated_at_utc_ms: category.updated_at_utc_ms,
                revision: category.revision,
                updated_by_device_id: category.updated_by_device_id.clone(),
            });
            new_categories += 1;
        }
    }

    let mut new_tasks = 0;
    let mut updated_tasks = 0;
    let mut kept_tasks = 0;
    for task in &package.tasks {
        let category_id = imported_category_ids
            .get(task.category_id.as_str())
            .ok_or_else(|| DataError("导入事项分类无效".into()))?;
        let local = transaction.query_row(
            "SELECT title, note, category_id, status, due_at_utc_ms, completed_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id
             FROM tasks WHERE id = ?1 AND deleted_at_utc_ms IS NULL",
            params![task.id],
            |row| Ok(LocalImportTask { title: row.get(0)?, note: row.get(1)?, category_id: row.get(2)?, status: row.get(3)?, due_at_utc_ms: row.get(4)?, completed_at_utc_ms: row.get(5)?, updated_at_utc_ms: row.get(6)?, revision: row.get(7)?, updated_by_device_id: row.get(8)? }),
        ).optional()?;
        match local {
            None => {
                transaction.execute(
                    "INSERT INTO tasks (id, title, note, category_id, status, due_at_utc_ms, recurrence_json, completed_at_utc_ms, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![task.id, task.title.trim(), task.note.trim(), category_id, task.status, task.due_at_utc_ms, task.recurrence_json, task.completed_at_utc_ms, task.created_at_utc_ms, task.updated_at_utc_ms, task.revision, task.updated_by_device_id],
                )?;
                new_tasks += 1;
            }
            Some(local) if incoming_task_wins(task, category_id, &local) => {
                transaction.execute(
                    "UPDATE tasks SET title = ?1, note = ?2, category_id = ?3, status = ?4, due_at_utc_ms = ?5, recurrence_json = ?6, completed_at_utc_ms = ?7, updated_at_utc_ms = ?8, revision = ?9, updated_by_device_id = ?10 WHERE id = ?11",
                    params![task.title.trim(), task.note.trim(), category_id, task.status, task.due_at_utc_ms, task.recurrence_json, task.completed_at_utc_ms, task.updated_at_utc_ms, task.revision, task.updated_by_device_id, task.id],
                )?;
                updated_tasks += 1;
            }
            Some(_) => kept_tasks += 1,
        }
    }
    Ok(ImportResultDto {
        source_file_name: source_file_name.to_string(),
        new_tasks,
        updated_tasks,
        kept_tasks,
        new_categories,
        updated_categories,
        kept_categories,
        snapshot_created: true,
    })
}

struct LocalImportTask {
    title: String,
    note: String,
    category_id: String,
    status: String,
    due_at_utc_ms: Option<i64>,
    completed_at_utc_ms: Option<i64>,
    updated_at_utc_ms: i64,
    revision: i64,
    updated_by_device_id: String,
}

fn incoming_task_wins(
    incoming: &ExportTaskDto,
    category_id: &str,
    local: &LocalImportTask,
) -> bool {
    use std::cmp::Ordering;
    match incoming.updated_at_utc_ms.cmp(&local.updated_at_utc_ms) {
        Ordering::Greater => return true,
        Ordering::Less => return false,
        Ordering::Equal => {}
    }
    match incoming.revision.cmp(&local.revision) {
        Ordering::Greater => return true,
        Ordering::Less => return false,
        Ordering::Equal => {}
    }
    let same_content = incoming.title == local.title
        && incoming.note == local.note
        && category_id == local.category_id
        && incoming.status == local.status
        && incoming.due_at_utc_ms == local.due_at_utc_ms
        && incoming.completed_at_utc_ms == local.completed_at_utc_ms;
    if same_content {
        return false;
    }
    incoming.updated_by_device_id > local.updated_by_device_id
}

fn validate_uuid(value: &str, label: &str) -> Result<(), DataError> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| DataError(format!("{label}无效")))
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn in_memory_store() -> DataStore {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&mut connection).unwrap();
        DataStore {
            connection: Mutex::new(connection),
            database_path: PathBuf::new(),
        }
    }

    #[test]
    fn corrupted_database_errors_do_not_expose_driver_details_or_claim_recovery() {
        let message = database_open_error("file is not a database: untrusted path").to_string();
        assert!(message.contains("未修改任何数据"));
        assert!(!message.contains("untrusted path"));
    }

    #[test]
    fn first_bootstrap_seeds_a_device_palette_and_eight_categories() {
        let bootstrap = in_memory_store().bootstrap().unwrap();
        assert!(Uuid::parse_str(&bootstrap.device_id).is_ok());
        assert_eq!(bootstrap.palette.len(), 24);
        assert_eq!(bootstrap.categories.len(), 8);
        assert_eq!(bootstrap.categories[0].name, "个人");
        assert_eq!(
            bootstrap.categories[0].default_key.as_deref(),
            Some("personal")
        );
        assert_eq!(bootstrap.categories[0].name_override, None);
        assert_eq!(bootstrap.categories[0].color, "#FF93C6");
        assert_eq!(bootstrap.categories[7].name, "其他");
        assert_eq!(
            bootstrap.categories[7].default_key.as_deref(),
            Some("other")
        );
        assert_eq!(bootstrap.categories[7].color, "#FF93C6");
    }
    #[test]
    fn theme_setting_is_validated_and_persisted() {
        let store = in_memory_store();
        assert_eq!(store.save_theme("dark").unwrap(), "dark");
        assert_eq!(store.bootstrap().unwrap().theme, "dark");
        assert!(store.save_theme("system").is_err());
    }

    #[test]
    fn startup_setting_defaults_to_enabled_and_persists() {
        let store = in_memory_store();
        assert!(store.startup_enabled().unwrap());
        assert!(!store.save_startup_enabled(false).unwrap());
        assert!(!store.bootstrap().unwrap().startup_enabled);
    }

    #[test]
    fn mcp_setting_defaults_to_enabled_and_persists() {
        let store = in_memory_store();
        assert!(store.mcp_enabled().unwrap());
        assert!(!store.save_mcp_enabled(false).unwrap());
        assert!(!store.bootstrap().unwrap().mcp_enabled);
        assert!(store.save_mcp_enabled(true).unwrap());
        assert!(store.bootstrap().unwrap().mcp_enabled);
    }

    #[test]
    fn window_state_is_optional_and_persists_geometry_and_mode() {
        let store = in_memory_store();
        assert!(store.window_state().unwrap().is_none());
        let state = WindowStateDto {
            x: -120,
            y: 80,
            width: 640,
            height: 720,
            mode: "mode-topmost".into(),
        };
        store.save_window_state(&state).unwrap();
        assert_eq!(store.window_state().unwrap().unwrap().mode, "mode-topmost");
        assert_eq!(store.window_state().unwrap().unwrap().x, -120);
    }
    #[test]
    fn locale_defaults_to_simplified_chinese_and_persists() {
        let store = in_memory_store();
        assert_eq!(store.bootstrap().unwrap().locale, "zh-CN");
        assert_eq!(store.save_locale("ja").unwrap(), "ja");
        assert_eq!(store.bootstrap().unwrap().locale, "ja");
        assert!(store.save_locale("unsupported").is_err());
    }

    #[test]
    fn installer_locale_marker_updates_locale_once() {
        let directory = std::env::temp_dir().join(format!("mylist-locale-test-{}-{}", std::process::id(), utc_now_ms()));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(INSTALLER_LOCALE_FILE), "ja").unwrap();
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&mut connection).unwrap();
        apply_installer_locale(&connection, &directory).unwrap();
        assert_eq!(read_locale(&connection).unwrap(), "ja");
        assert!(!directory.join(INSTALLER_LOCALE_FILE).exists());
        fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn rerunning_migration_preserves_existing_identity_and_categories() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&mut connection).unwrap();
        let first = read_bootstrap(&connection).unwrap();
        initialize_database(&mut connection).unwrap();
        let second = read_bootstrap(&connection).unwrap();
        assert_eq!(first.device_id, second.device_id);
        assert_eq!(first.categories.len(), second.categories.len());
    }

    #[test]
    fn tasks_keep_distinct_ids_and_support_the_complete_restore_delete_lifecycle() {
        let store = in_memory_store();
        let category_id = store.bootstrap().unwrap().categories[0].id.clone();
        let first = store
            .create_task(CreateTaskInput {
                title: "同名事项".into(),
                note: "备注".into(),
                category_id: category_id.clone(),
                due_at_utc_ms: Some(1_786_860_000_000),
            })
            .unwrap();
        let second = store
            .create_task(CreateTaskInput {
                title: "同名事项".into(),
                note: "".into(),
                category_id,
                due_at_utc_ms: None,
            })
            .unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(first.due_at_utc_ms, Some(1_786_860_000_000));
        assert_eq!(second.due_at_utc_ms, None);
        assert_eq!(store.list_tasks("todo").unwrap().len(), 2);
        assert_eq!(
            store
                .set_task_status(&first.id, "completed")
                .unwrap()
                .status,
            "completed"
        );
        assert_eq!(store.list_tasks("todo").unwrap().len(), 1);
        assert_eq!(
            store.set_task_status(&first.id, "todo").unwrap().status,
            "todo"
        );
        store.delete_task(&first.id).unwrap();
        assert!(store.get_task(&first.id).is_err());
        assert_eq!(store.list_tasks("todo").unwrap().len(), 1);
    }

    #[test]
    fn mcp_task_writes_are_idempotent_and_revision_guarded() {
        let store = in_memory_store();
        let category_id = store.bootstrap().unwrap().categories[0].id.clone();
        let input = CreateTaskInput {
            title: "MCP 测试事项".into(),
            note: "不会重复创建".into(),
            category_id: category_id.clone(),
            due_at_utc_ms: None,
        };
        let created = store
            .mcp_create_task("request-create-1", input.clone(), None)
            .unwrap();
        let repeated = store
            .mcp_create_task("request-create-1", input, None)
            .unwrap();
        assert_eq!(created.id, repeated.id);
        assert_eq!(store.list_tasks("todo").unwrap().len(), 1);

        let completed = store
            .mcp_set_task_status(
                "request-complete-1",
                &created.id,
                "completed",
                created.revision,
            )
            .unwrap();
        assert_eq!(completed.status, "completed");
        assert!(store
            .mcp_set_task_status(
                "request-restore-conflict",
                &created.id,
                "todo",
                created.revision
            )
            .is_err());
        let restored = store
            .mcp_set_task_status("request-restore-1", &created.id, "todo", completed.revision)
            .unwrap();
        assert_eq!(restored.status, "todo");
    }

    #[test]
    fn mcp_category_writes_are_idempotent_and_delete_is_preview_only() {
        let store = in_memory_store();
        let palette = store.bootstrap().unwrap().palette;
        let created = store
            .mcp_create_category(
                "request-category-create",
                "Agent 分类",
                Some(&palette[0].id),
            )
            .unwrap();
        let repeated = store
            .mcp_create_category("request-category-create", "不会重复", Some(&palette[1].id))
            .unwrap();
        assert_eq!(created.id, repeated.id);
        assert_eq!(
            store
                .bootstrap()
                .unwrap()
                .categories
                .iter()
                .filter(|category| category.name == "Agent 分类")
                .count(),
            1
        );

        let updated = store
            .mcp_update_category(
                "request-category-update",
                UpdateCategoryInput {
                    id: created.id.clone(),
                    name: "Agent 分类已改名".into(),
                    color_id: palette[1].id.clone(),
                },
                created.revision,
            )
            .unwrap();
        assert!(store
            .mcp_update_category(
                "request-category-conflict",
                UpdateCategoryInput {
                    id: created.id.clone(),
                    name: "不应写入".into(),
                    color_id: palette[2].id.clone(),
                },
                created.revision,
            )
            .is_err());
        store
            .create_task(CreateTaskInput {
                title: "引用分类的测试事项".into(),
                note: "".into(),
                category_id: updated.id.clone(),
                due_at_utc_ms: None,
            })
            .unwrap();
        let preview = store.mcp_prepare_delete_category(&updated.id).unwrap();
        assert_eq!(preview.task_count, 1);
        assert!(preview
            .migration_targets
            .iter()
            .all(|target| target.id != updated.id));
        assert_eq!(
            store
                .get_task(&store.list_tasks("todo").unwrap()[0].id)
                .unwrap()
                .category_id,
            updated.id
        );
    }

    #[test]
    fn plaintext_export_includes_active_business_data_but_no_delete_markers() {
        let store = in_memory_store();
        let category_id = store.bootstrap().unwrap().categories[0].id.clone();
        let kept = store
            .create_task(CreateTaskInput {
                title: "保留事项".into(),
                note: "导出备注".into(),
                category_id: category_id.clone(),
                due_at_utc_ms: None,
            })
            .unwrap();
        let removed = store
            .create_task(CreateTaskInput {
                title: "已删除事项".into(),
                note: "".into(),
                category_id,
                due_at_utc_ms: None,
            })
            .unwrap();
        store.delete_task(&removed.id).unwrap();
        let snapshot = store.plaintext_export_snapshot().unwrap();
        assert_eq!(snapshot.schema_version, 1);
        assert_eq!(snapshot.palette.len(), 24);
        assert_eq!(snapshot.categories.len(), 8);
        assert_eq!(snapshot.tasks.len(), 1);
        assert_eq!(snapshot.tasks[0].id, kept.id);
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("deletedAtUtcMs"));
        assert!(!json.contains("已删除事项"));
    }

    #[test]
    fn overwrite_restore_replaces_business_data_but_keeps_device_settings() {
        let directory = std::env::temp_dir().join(format!("mylist-overwrite-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let mut connection = Connection::open_in_memory().unwrap();
        initialize_database(&mut connection).unwrap();
        let store = DataStore {
            connection: Mutex::new(connection),
            database_path: directory.join("mylist.db"),
        };
        let snapshot = store.plaintext_export_snapshot().unwrap();
        assert!(!store.save_startup_enabled(false).unwrap());
        let category_id = store.bootstrap().unwrap().categories[0].id.clone();
        store
            .create_task(CreateTaskInput {
                title: "本机临时事项".into(),
                note: "会被覆盖".into(),
                category_id,
                due_at_utc_ms: None,
            })
            .unwrap();

        let result = store
            .replace_plaintext_package(&snapshot, "restore.dtodo.json")
            .unwrap();
        assert_eq!(result.new_tasks, 0);
        assert_eq!(store.list_tasks("todo").unwrap().len(), 0);
        assert_eq!(store.bootstrap().unwrap().categories.len(), 8);
        assert!(!store.startup_enabled().unwrap());
        drop(store);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn tasks_validate_title_note_and_category_before_writing() {
        let store = in_memory_store();
        let category_id = store.bootstrap().unwrap().categories[0].id.clone();
        assert!(store
            .create_task(CreateTaskInput {
                title: "   ".into(),
                note: "".into(),
                category_id: category_id.clone(),
                due_at_utc_ms: None,
            })
            .is_err());
        assert!(store
            .create_task(CreateTaskInput {
                title: "标题".into(),
                note: "x".repeat(2_001),
                category_id: category_id.clone(),
                due_at_utc_ms: None,
            })
            .is_err());
        assert!(store
            .create_task(CreateTaskInput {
                title: "标题".into(),
                note: "".into(),
                category_id: "missing".into(),
                due_at_utc_ms: None,
            })
            .is_err());
        assert!(store
            .create_task(CreateTaskInput {
                title: "标题".into(),
                note: "".into(),
                category_id,
                due_at_utc_ms: None,
            })
            .is_ok());
    }

    #[test]
    fn categories_can_be_created_updated_and_default_to_an_unused_palette_color() {
        let store = in_memory_store();
        let before = store.bootstrap().unwrap();
        let created = store
            .create_category(CreateCategoryInput {
                name: "新分类".into(),
            })
            .unwrap();
        assert_eq!(created.name, "新分类");
        assert_eq!(created.color_id, before.palette[0].id);
        assert!(!before
            .categories
            .iter()
            .any(|category| category.color_id == created.color_id));
        let palette_color = before.palette[0].id.clone();
        let updated = store
            .update_category(UpdateCategoryInput {
                id: created.id,
                name: "已重命名".into(),
                color_id: palette_color,
            })
            .unwrap();
        assert_eq!(updated.name, "已重命名");
        assert_eq!(
            store
                .create_category(CreateCategoryInput {
                    name: "已重命名".into(),
                })
                .unwrap()
                .name,
            "已重命名-1"
        );
    }

    #[test]
    fn restoring_defaults_only_adds_missing_defaults_and_preserves_custom_categories() {
        let store = in_memory_store();
        let personal = store.bootstrap().unwrap().categories[0].clone();
        store.delete_category(&personal.id, None).unwrap();
        let custom = store
            .create_category(CreateCategoryInput {
                name: "自定义分类".into(),
            })
            .unwrap();

        store.restore_default_categories().unwrap();
        let categories = store.bootstrap().unwrap().categories;
        assert_eq!(categories.len(), 9);
        assert!(categories
            .iter()
            .any(|category| { category.name == "个人" && category.color == "#FF93C6" }));
        assert!(categories
            .iter()
            .any(|category| { category.id == custom.id && category.name == "自定义分类" }));
    }

    #[test]
    fn restoring_defaults_treats_a_canonical_name_as_an_existing_default() {
        let store = in_memory_store();
        let personal = store.bootstrap().unwrap().categories[0].clone();
        store.delete_category(&personal.id, None).unwrap();
        let replacement = store
            .create_category(CreateCategoryInput {
                name: "个人".into(),
            })
            .unwrap();

        store.restore_default_categories().unwrap();
        let categories = store.bootstrap().unwrap().categories;
        assert_eq!(
            categories.iter().filter(|item| item.name == "个人").count(),
            1
        );
        assert!(categories.iter().any(|item| item.id == replacement.id));
    }

    #[test]
    fn restoring_a_renamed_default_preserves_the_custom_category_and_its_tasks() {
        let store = in_memory_store();
        let initial = store.bootstrap().unwrap();
        let personal = initial.categories[0].clone();
        let changed_color = initial.palette[0].id.clone();
        let task = store
            .create_task(CreateTaskInput {
                title: "保留给自定义分类的事项".into(),
                note: "".into(),
                category_id: personal.id.clone(),
                due_at_utc_ms: None,
            })
            .unwrap();
        store
            .update_category(UpdateCategoryInput {
                id: personal.id.clone(),
                name: "我的个人事项".into(),
                color_id: changed_color.clone(),
            })
            .unwrap();

        store.restore_default_categories().unwrap();
        let categories = store.bootstrap().unwrap().categories;
        let restored = categories
            .iter()
            .find(|category| category.id == personal.id)
            .unwrap();
        let custom = categories
            .iter()
            .find(|category| category.name == "我的个人事项")
            .unwrap();
        assert_eq!(restored.default_key.as_deref(), Some("personal"));
        assert_eq!(restored.name_override, None);
        assert_eq!(restored.color, "#FF93C6");
        assert_eq!(custom.default_key, None);
        assert_eq!(custom.color_id, changed_color);
        assert_eq!(store.get_task(&task.id).unwrap().category_id, custom.id);
    }

    #[test]
    fn default_categories_merge_by_stable_key_even_when_export_ids_differ() {
        let store = in_memory_store();
        let mut package = store.plaintext_export_snapshot().unwrap();
        package.categories[0].id = Uuid::new_v4().to_string();
        package.categories[0].name = "Personal".into();

        {
            let mut connection = store.connection.lock().unwrap();
            let preview =
                build_import_preview(&connection, &package, "english.dtodo.json").unwrap();
            assert_eq!(preview.new_categories, 0);
            assert_eq!(preview.kept_categories, 8);
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let result =
                merge_plaintext_package(&transaction, &package, "english.dtodo.json").unwrap();
            transaction.commit().unwrap();
            assert_eq!(result.new_categories, 0);
            assert_eq!(result.kept_categories, 8);
        }
        assert_eq!(store.bootstrap().unwrap().categories.len(), 8);
    }

    #[test]
    fn deleting_a_referenced_category_requires_and_atomically_applies_a_migration_target() {
        let store = in_memory_store();
        let categories = store.bootstrap().unwrap().categories;
        let source = categories[0].clone();
        let target = categories[1].clone();
        let task = store
            .create_task(CreateTaskInput {
                title: "待迁移事项".into(),
                note: "".into(),
                category_id: source.id.clone(),
                due_at_utc_ms: None,
            })
            .unwrap();
        assert!(store.delete_category(&source.id, None).is_err());
        store.delete_category(&source.id, Some(&target.id)).unwrap();
        let migrated = store.get_task(&task.id).unwrap();
        assert_eq!(migrated.category_id, target.id);
        assert!(!store
            .bootstrap()
            .unwrap()
            .categories
            .iter()
            .any(|category| category.id == source.id));
    }

    #[test]
    fn mcp_create_and_update_support_recurrence() {
        let store = in_memory_store();
        let category_id = store.bootstrap().unwrap().categories[0].id.clone();
        let due_at = 1_786_860_000_000;
        let recurrence = RecurrenceConfig {
            interval: 1,
            unit: "week".into(),
            action: "create_new".into(),
            base_title: "每周复盘".into(),
        };
        let created = store
            .mcp_create_task(
                "recurrence-create",
                CreateTaskInput {
                    title: "每周复盘".into(),
                    note: "MCP 创建".into(),
                    category_id: category_id.clone(),
                    due_at_utc_ms: Some(due_at),
                },
                Some(recurrence),
            )
            .unwrap();
        assert!(created.recurrence_json.is_some());

        let updated = store
            .mcp_update_task(
                "recurrence-disable",
                UpdateTaskInput {
                    id: created.id,
                    title: "每周复盘".into(),
                    note: "MCP 更新".into(),
                    category_id,
                    due_at_utc_ms: Some(due_at),
                },
                created.revision,
                Some(None),
            )
            .unwrap();
        assert_eq!(updated.recurrence_json, None);
    }
}
