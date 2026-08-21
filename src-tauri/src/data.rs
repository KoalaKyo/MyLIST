use std::{
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskInput {
    pub title: String,
    pub note: String,
    pub category_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskInput {
    pub id: String,
    pub title: String,
    pub note: String,
    pub category_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDto {
    pub id: String,
    pub title: String,
    pub note: String,
    pub category_id: String,
    pub category_name: String,
    pub category_color: String,
    pub status: String,
    pub due_at_utc_ms: Option<i64>,
    pub created_at_utc_ms: i64,
    pub updated_at_utc_ms: i64,
    pub completed_at_utc_ms: Option<i64>,
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
            "INSERT INTO tasks (id, title, note, category_id, status, created_at_utc_ms, updated_at_utc_ms, revision, updated_by_device_id)
             VALUES (?1, ?2, ?3, ?4, 'todo', ?5, ?5, 1, ?6)",
            params![id, title, input.note.trim(), input.category_id, now, device_id],
        )?;
        transaction.commit()?;
        read_task(&connection, &id)
    }

    pub fn update_task(&self, input: UpdateTaskInput) -> Result<TaskDto, DataError> {
        let title = validate_task_input(&input.title, &input.note)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_active_category(&transaction, &input.category_id)?;
        let device_id = read_device_id(&transaction)?;
        let changed = transaction.execute(
            "UPDATE tasks SET title = ?1, note = ?2, category_id = ?3, updated_at_utc_ms = ?4,
             revision = revision + 1, updated_by_device_id = ?5 WHERE id = ?6 AND deleted_at_utc_ms IS NULL",
            params![title, input.note.trim(), input.category_id, utc_now_ms(), device_id, input.id],
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

    pub fn delete_task(&self, id: &str) -> Result<(), DataError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| DataError("本地数据连接不可用".into()))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "UPDATE tasks SET deleted_at_utc_ms = ?1, updated_at_utc_ms = ?1, revision = revision + 1,
             updated_by_device_id = ?2 WHERE id = ?3 AND deleted_at_utc_ms IS NULL",
            params![utc_now_ms(), read_device_id(&transaction)?, id],
        )?;
        if changed != 1 {
            return Err(DataError("事项不存在或已删除".into()));
        }
        transaction.commit()?;
        Ok(())
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
             updated_by_device_id TEXT NOT NULL, deleted_at_utc_ms INTEGER, sort_order INTEGER NOT NULL DEFAULT 0, UNIQUE(name));
         CREATE TABLE IF NOT EXISTS tasks (
             id TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL, note TEXT NOT NULL DEFAULT '', category_id TEXT NOT NULL REFERENCES categories(id),
             status TEXT NOT NULL CHECK(status IN ('todo', 'completed')), due_at_utc_ms INTEGER,
             completed_at_utc_ms INTEGER, created_at_utc_ms INTEGER NOT NULL, updated_at_utc_ms INTEGER NOT NULL,
             revision INTEGER NOT NULL DEFAULT 1, updated_by_device_id TEXT NOT NULL, deleted_at_utc_ms INTEGER);",
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
    transaction.execute(
        "INSERT OR IGNORE INTO schema_migrations (version, applied_at_utc_ms) VALUES (2, ?1)",
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

fn read_task(connection: &Connection, id: &str) -> Result<TaskDto, DataError> {
    connection.query_row(
        "SELECT tasks.id, tasks.title, tasks.note, tasks.category_id, categories.name, palette_colors.value, tasks.status,
         tasks.due_at_utc_ms, tasks.created_at_utc_ms, tasks.updated_at_utc_ms, tasks.completed_at_utc_ms FROM tasks
         JOIN categories ON categories.id = tasks.category_id JOIN palette_colors ON palette_colors.id = categories.color_id
         WHERE tasks.id = ?1 AND tasks.deleted_at_utc_ms IS NULL",
        params![id], task_from_row,
    ).optional()?.ok_or_else(|| DataError("事项不存在或已删除".into()))
}

fn read_tasks(connection: &Connection, status: &str) -> Result<Vec<TaskDto>, DataError> {
    let ordering = if status == "todo" {
        "CASE WHEN due_at_utc_ms IS NOT NULL AND due_at_utc_ms < ?2 THEN 0 ELSE 1 END, CASE WHEN due_at_utc_ms IS NULL THEN 1 ELSE 0 END, due_at_utc_ms ASC, tasks.updated_at_utc_ms DESC, tasks.id"
    } else {
        "tasks.completed_at_utc_ms DESC, tasks.updated_at_utc_ms DESC, tasks.id"
    };
    let query = format!(
        "SELECT tasks.id, tasks.title, tasks.note, tasks.category_id, categories.name, palette_colors.value, tasks.status,
         tasks.due_at_utc_ms, tasks.created_at_utc_ms, tasks.updated_at_utc_ms, tasks.completed_at_utc_ms FROM tasks
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
        category_color: row.get(5)?,
        status: row.get(6)?,
        due_at_utc_ms: row.get(7)?,
        created_at_utc_ms: row.get(8)?,
        updated_at_utc_ms: row.get(9)?,
        completed_at_utc_ms: row.get(10)?,
    })
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

    #[test]
    fn tasks_keep_distinct_ids_and_support_the_complete_restore_delete_lifecycle() {
        let store = in_memory_store();
        let category_id = store.bootstrap().unwrap().categories[0].id.clone();
        let first = store
            .create_task(CreateTaskInput {
                title: "同名事项".into(),
                note: "备注".into(),
                category_id: category_id.clone(),
            })
            .unwrap();
        let second = store
            .create_task(CreateTaskInput {
                title: "同名事项".into(),
                note: "".into(),
                category_id,
            })
            .unwrap();
        assert_ne!(first.id, second.id);
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
    fn tasks_validate_title_note_and_category_before_writing() {
        let store = in_memory_store();
        let category_id = store.bootstrap().unwrap().categories[0].id.clone();
        assert!(store
            .create_task(CreateTaskInput {
                title: "   ".into(),
                note: "".into(),
                category_id: category_id.clone()
            })
            .is_err());
        assert!(store
            .create_task(CreateTaskInput {
                title: "标题".into(),
                note: "x".repeat(2_001),
                category_id: category_id.clone()
            })
            .is_err());
        assert!(store
            .create_task(CreateTaskInput {
                title: "标题".into(),
                note: "".into(),
                category_id: "missing".into()
            })
            .is_err());
        assert!(store
            .create_task(CreateTaskInput {
                title: "标题".into(),
                note: "".into(),
                category_id
            })
            .is_ok());
    }
}
