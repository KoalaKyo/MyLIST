use std::{
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const DATABASE_FILE: &str = "mylist.sqlite3";
const THEME_KEY: &str = "theme";

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

const DEFAULT_CATEGORIES: [(&str, usize); 7] = [
    ("工作", 0),
    ("个人", 3),
    ("团队", 6),
    ("生活", 2),
    ("财务", 5),
    ("学习", 1),
    ("出行", 4),
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaletteColorDto {
    pub id: String,
    pub row: u8,
    pub column: u8,
    pub value: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDto {
    pub id: String,
    pub name: String,
    pub color_id: String,
    pub color: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapDto {
    pub device_id: String,
    pub theme: String,
    pub categories: Vec<CategoryDto>,
    pub palette: Vec<PaletteColorDto>,
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
        let mut connection = Connection::open(&database_path)?;
        initialize_database(&mut connection)?;
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
}

fn initialize_database(connection: &mut Connection) -> Result<(), DataError> {
    connection.execute_batch(
        "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
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
             updated_by_device_id TEXT NOT NULL, deleted_at_utc_ms INTEGER, sort_order INTEGER NOT NULL DEFAULT 0, UNIQUE(name));",
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
        for (sort_order, (name, color_column)) in DEFAULT_CATEGORIES.iter().enumerate() {
            transaction.execute(
                "INSERT INTO categories (id, name, color_id, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id, sort_order) VALUES (?1, ?2, ?3, ?4, ?4, 1, ?5, ?6)",
                params![Uuid::new_v4().to_string(), name, palette_id(1, *color_column), now, device_id, sort_order as i64],
            )?;
        }
    }
    transaction.execute(
        "INSERT OR IGNORE INTO schema_migrations (version, applied_at_utc_ms) VALUES (1, ?1)",
        params![now],
    )?;
    transaction.commit()?;
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
        "SELECT categories.id, categories.name, categories.color_id, palette_colors.value FROM categories JOIN palette_colors ON palette_colors.id = categories.color_id WHERE categories.deleted_at_utc_ms IS NULL ORDER BY categories.sort_order, categories.created_at_utc_ms, categories.id",
    )?;
    let categories = category_statement
        .query_map([], |row| {
            Ok(CategoryDto {
                id: row.get(0)?,
                name: row.get(1)?,
                color_id: row.get(2)?,
                color: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BootstrapDto {
        device_id,
        theme,
        categories,
        palette,
    })
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
    fn first_bootstrap_seeds_a_device_palette_and_seven_categories() {
        let bootstrap = in_memory_store().bootstrap().unwrap();
        assert!(Uuid::parse_str(&bootstrap.device_id).is_ok());
        assert_eq!(bootstrap.palette.len(), 24);
        assert_eq!(bootstrap.categories.len(), 7);
        assert_eq!(bootstrap.categories[0].name, "工作");
        assert_eq!(bootstrap.categories[0].color, "#8CB9FF");
    }
    #[test]
    fn theme_setting_is_validated_and_persisted() {
        let store = in_memory_store();
        assert_eq!(store.save_theme("dark").unwrap(), "dark");
        assert_eq!(store.bootstrap().unwrap().theme, "dark");
        assert!(store.save_theme("system").is_err());
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
}
