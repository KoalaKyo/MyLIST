//! Read-only MCP protocol bridge for AI-2.
//!
//! The bridge speaks newline-delimited JSON-RPC on the private named pipe
//! owned by `mcp_service`. It deliberately exposes only read operations in
//! this stage; all data access goes through `DataStore` methods.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    data::{CreateTaskInput, RecurrenceConfig, UpdateCategoryInput, UpdateTaskInput},
    mcp, mcp_confirmation, mcp_transfer, DataStore,
};

const DEFAULT_PAGE_SIZE: usize = 20;
const MAX_PAGE_SIZE: usize = 100;
const SERVER_NAME: &str = "MyLIST";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: &'static str,
    data: Option<Value>,
}

fn success(id: Option<Value>, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    }
}

fn protocol_error(id: Option<Value>, code: i32, message: &'static str) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message,
            data: None,
        }),
    }
}

fn tool_error(id: Option<Value>, code: mcp::McpErrorCode) -> JsonRpcResponse {
    let error = mcp::error(code, matches!(code, mcp::McpErrorCode::ServiceUnavailable));
    success(
        id,
        json!({
            "isError": true,
            "content": [{"type": "text", "text": error.code}],
            "structuredContent": error,
        }),
    )
}

fn page_params(params: &Value) -> Result<(usize, usize), mcp::McpErrorCode> {
    let page = params.get("page").and_then(Value::as_u64).unwrap_or(0) as usize;
    let page_size = params
        .get("pageSize")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_PAGE_SIZE as u64) as usize;
    if page_size == 0 || page_size > MAX_PAGE_SIZE {
        return Err(mcp::McpErrorCode::InvalidArgument);
    }
    Ok((page, page_size))
}

fn paged<T: Serialize>(items: Vec<T>, page: usize, page_size: usize) -> Value {
    let total = items.len();
    let start = page.saturating_mul(page_size);
    let values = if start >= total {
        Vec::new()
    } else {
        items
            .into_iter()
            .skip(start)
            .take(page_size)
            .collect::<Vec<_>>()
    };
    let has_more = start.saturating_add(values.len()) < total;
    json!({
        "items": values,
        "page": page,
        "pageSize": page_size,
        "total": total,
        "hasMore": has_more,
        "nextPage": has_more.then_some(page + 1),
    })
}

fn read_tool(app: &AppHandle, name: &str, params: &Value) -> Result<Value, mcp::McpErrorCode> {
    let store = app
        .try_state::<DataStore>()
        .ok_or(mcp::McpErrorCode::ServiceUnavailable)?;
    match name {
        "mylist_get_overview" => {
            let bootstrap = store
                .bootstrap()
                .map_err(|_| mcp::McpErrorCode::InternalError)?;
            let todo = store
                .list_tasks("todo")
                .map_err(|_| mcp::McpErrorCode::InternalError)?;
            let completed = store
                .list_tasks("completed")
                .map_err(|_| mcp::McpErrorCode::InternalError)?;
            Ok(json!({
                "status": "online",
                "protocolVersion": mcp::MCP_PROTOCOL_VERSION,
                "server": {"name": SERVER_NAME, "version": SERVER_VERSION},
                "counts": {"todo": todo.len(), "completed": completed.len(), "categories": bootstrap.categories.len()},
            }))
        }
        "mylist_list_tasks" => {
            let status = params
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("todo");
            if !matches!(status, "todo" | "completed") {
                return Err(mcp::McpErrorCode::InvalidArgument);
            }
            let category_id = params.get("categoryId").and_then(Value::as_str);
            let due_from = params.get("dueFromUtcMs").and_then(Value::as_i64);
            let due_to = params.get("dueToUtcMs").and_then(Value::as_i64);
            let updated_since = params.get("updatedSinceUtcMs").and_then(Value::as_i64);
            let tasks = store
                .list_tasks(status)
                .map_err(|_| mcp::McpErrorCode::InternalError)?
                .into_iter()
                .filter(|task| category_id.map_or(true, |id| task.category_id == id))
                .filter(|task| {
                    due_from.map_or(true, |from| {
                        task.due_at_utc_ms.is_some_and(|due| due >= from)
                    })
                })
                .filter(|task| {
                    due_to.map_or(true, |to| task.due_at_utc_ms.is_some_and(|due| due <= to))
                })
                .filter(|task| updated_since.map_or(true, |since| task.updated_at_utc_ms > since))
                .collect::<Vec<_>>();
            let (page, page_size) = page_params(params)?;
            Ok(paged(tasks, page, page_size))
        }
        "mylist_get_task" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or(mcp::McpErrorCode::InvalidArgument)?;
            let task = store.get_task(id).map_err(|error| {
                if error.to_string().contains("不存在") {
                    mcp::McpErrorCode::NotFound
                } else {
                    mcp::McpErrorCode::InternalError
                }
            })?;
            serde_json::to_value(task).map_err(|_| mcp::McpErrorCode::InternalError)
        }
        "mylist_list_categories" => {
            let bootstrap = store
                .bootstrap()
                .map_err(|_| mcp::McpErrorCode::InternalError)?;
            let todo = store
                .list_tasks("todo")
                .map_err(|_| mcp::McpErrorCode::InternalError)?;
            let completed = store
                .list_tasks("completed")
                .map_err(|_| mcp::McpErrorCode::InternalError)?;
            let items = bootstrap
                .categories
                .into_iter()
                .map(|category| {
                    let todo_count = todo
                        .iter()
                        .filter(|task| task.category_id == category.id)
                        .count();
                    let completed_count = completed
                        .iter()
                        .filter(|task| task.category_id == category.id)
                        .count();
                    json!({
                        "id": category.id,
                        "name": category.name,
                        "defaultKey": category.default_key,
                        "nameOverride": category.name_override,
                        "colorId": category.color_id,
                        "color": category.color,
                        "taskCounts": {"todo": todo_count, "completed": completed_count},
                    })
                })
                .collect::<Vec<_>>();
            let (page, page_size) = page_params(params)?;
            Ok(paged(items, page, page_size))
        }
        "mylist_get_palette" | "mylist_list_palette" => {
            let bootstrap = store
                .bootstrap()
                .map_err(|_| mcp::McpErrorCode::InternalError)?;
            let (page, page_size) = page_params(params)?;
            Ok(paged(bootstrap.palette, page, page_size))
        }
        _ => Err(mcp::McpErrorCode::NotFound),
    }
}

fn write_error(error: impl std::fmt::Display) -> mcp::McpErrorCode {
    let text = error.to_string();
    if text.contains("已被更新") {
        mcp::McpErrorCode::Conflict
    } else if text.contains("不存在") {
        mcp::McpErrorCode::NotFound
    } else if text.contains("标题")
        || text.contains("备注")
        || text.contains("分类")
        || text.contains("截止")
    {
        mcp::McpErrorCode::InvalidArgument
    } else {
        mcp::McpErrorCode::InternalError
    }
}

fn request_meta(params: &Value) -> Result<mcp::McpRequestMeta, mcp::McpErrorCode> {
    let meta = mcp::McpRequestMeta {
        request_id: params
            .get("requestId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        protocol_version: params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    };
    mcp::validate_request_meta(&meta)?;
    Ok(meta)
}

fn required_string<'a>(params: &'a Value, key: &str) -> Result<&'a str, mcp::McpErrorCode> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(mcp::McpErrorCode::InvalidArgument)
}

fn required_revision(params: &Value) -> Result<i64, mcp::McpErrorCode> {
    params
        .get("expectedRevision")
        .and_then(Value::as_i64)
        .filter(|value| *value >= 1)
        .ok_or(mcp::McpErrorCode::InvalidArgument)
}

fn delete_fingerprint(
    operation: &str,
    id: &str,
    expected_revision: i64,
    target_category_id: Option<&str>,
) -> String {
    mcp_confirmation::fingerprint(
        &json!({"operation": operation, "id": id, "expectedRevision": expected_revision, "targetCategoryId": target_category_id}),
    )
}

fn request_confirmation(app: &AppHandle, params: &Value) -> Result<Value, mcp::McpErrorCode> {
    request_meta(params)?;
    let store = app
        .try_state::<DataStore>()
        .ok_or(mcp::McpErrorCode::ServiceUnavailable)?;
    let operation = required_string(params, "operation")?;
    let id = required_string(params, "id")?;
    let expected_revision = required_revision(params)?;
    let (scope, fingerprint, preview) = match operation {
        "delete_task" => {
            let task = store.mcp_prepare_delete_task(id).map_err(write_error)?;
            if task.revision != expected_revision {
                return Err(mcp::McpErrorCode::Conflict);
            }
            (
                format!("delete-task:{id}"),
                delete_fingerprint(operation, id, expected_revision, None),
                json!({"task": task}),
            )
        }
        "delete_category" => {
            let target_category_id = params
                .get("targetCategoryId")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty());
            let category = store.mcp_prepare_delete_category(id).map_err(write_error)?;
            if category.category.revision != expected_revision {
                return Err(mcp::McpErrorCode::Conflict);
            }
            if category.task_count > 0 && target_category_id.is_none() {
                return Err(mcp::McpErrorCode::InvalidArgument);
            }
            if let Some(target) = target_category_id {
                if target == id
                    || !category
                        .migration_targets
                        .iter()
                        .any(|candidate| candidate.id == target)
                {
                    return Err(mcp::McpErrorCode::InvalidArgument);
                }
            }
            (
                format!("delete-category:{id}"),
                delete_fingerprint(operation, id, expected_revision, target_category_id),
                json!({"category": category.category, "taskCount": category.task_count, "targetCategoryId": target_category_id}),
            )
        }
        _ => return Err(mcp::McpErrorCode::InvalidArgument),
    };
    let confirmation = app
        .state::<mcp_confirmation::McpConfirmationState>()
        .request(operation, scope, fingerprint)?;
    let _ = app.emit("mylist-mcp-confirmation-requested", json!({"token": confirmation.token, "operation": confirmation.operation, "expiresAtUtcMs": confirmation.expires_at_utc_ms, "preview": preview}));
    serde_json::to_value(confirmation).map_err(|_| mcp::McpErrorCode::InternalError)
}

fn transfer_prepare(
    app: &AppHandle,
    name: &str,
    params: &Value,
) -> Result<Value, mcp::McpErrorCode> {
    request_meta(params)?;
    let operation = match name {
        "mylist_export_prepare" => match required_string(params, "format")? {
            "plaintext" => "export_plaintext",
            "encrypted" => "export_encrypted",
            _ => return Err(mcp::McpErrorCode::InvalidArgument),
        },
        "mylist_import_prepare" => match required_string(params, "operation")? {
            "merge" => "import_merge",
            "replace" => "import_replace",
            _ => return Err(mcp::McpErrorCode::InvalidArgument),
        },
        _ => return Err(mcp::McpErrorCode::NotFound),
    };
    let transfer = app
        .state::<mcp_transfer::McpTransferState>()
        .request(operation)?;
    let _ = app.emit(
        "mylist-mcp-transfer-requested",
        json!({
            "operationId": transfer.operation_id,
            "operation": transfer.operation,
        }),
    );
    serde_json::to_value(transfer).map_err(|_| mcp::McpErrorCode::InternalError)
}

fn transfer_status(app: &AppHandle, params: &Value) -> Result<Value, mcp::McpErrorCode> {
    let operation_id = required_string(params, "operationId")?;
    let status = app
        .state::<mcp_transfer::McpTransferState>()
        .get(operation_id)?;
    serde_json::to_value(status).map_err(|_| mcp::McpErrorCode::InternalError)
}

fn write_tool(app: &AppHandle, name: &str, params: &Value) -> Result<Value, mcp::McpErrorCode> {
    let store = app
        .try_state::<DataStore>()
        .ok_or(mcp::McpErrorCode::ServiceUnavailable)?;
    let meta = request_meta(params)?;
    let title = || {
        params
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let note = || {
        params
            .get("note")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let category_id = || {
        params
            .get("categoryId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let due_at_utc_ms = params.get("dueAtUtcMs").and_then(Value::as_i64);
    let recurrence = match params.get("recurrence") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(value) => Some(Some(
            serde_json::from_value::<RecurrenceConfig>(value.clone())
                .map_err(|_| mcp::McpErrorCode::InvalidArgument)?,
        )),
    };
    let result = match name {
        "mylist_create_task" => serde_json::to_value(
            store
                .mcp_create_task(
                    &meta.request_id,
                    CreateTaskInput {
                        title: title(),
                        note: note(),
                        category_id: category_id(),
                        due_at_utc_ms,
                    },
                    recurrence.flatten(),
                )
                .map_err(write_error)?,
        )
        .map_err(|_| mcp::McpErrorCode::InternalError)?,
        "mylist_update_task" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or(mcp::McpErrorCode::InvalidArgument)?;
            let expected_revision = params
                .get("expectedRevision")
                .and_then(Value::as_i64)
                .filter(|value| *value >= 1)
                .ok_or(mcp::McpErrorCode::InvalidArgument)?;
            serde_json::to_value(
                store
                    .mcp_update_task(
                        &meta.request_id,
                        UpdateTaskInput {
                            id: id.to_string(),
                            title: title(),
                            note: note(),
                            category_id: category_id(),
                            due_at_utc_ms,
                        },
                        expected_revision,
                        recurrence,
                    )
                    .map_err(write_error)?,
            )
            .map_err(|_| mcp::McpErrorCode::InternalError)?
        }
        "mylist_complete_task" | "mylist_restore_task" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or(mcp::McpErrorCode::InvalidArgument)?;
            let expected_revision = params
                .get("expectedRevision")
                .and_then(Value::as_i64)
                .filter(|value| *value >= 1)
                .ok_or(mcp::McpErrorCode::InvalidArgument)?;
            let status = if name == "mylist_complete_task" {
                "completed"
            } else {
                "todo"
            };
            serde_json::to_value(
                store
                    .mcp_set_task_status(&meta.request_id, id, status, expected_revision)
                    .map_err(write_error)?,
            )
            .map_err(|_| mcp::McpErrorCode::InternalError)?
        }
        "mylist_create_category" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let color_id = params.get("colorId").and_then(Value::as_str);
            serde_json::to_value(
                store
                    .mcp_create_category(&meta.request_id, name, color_id)
                    .map_err(write_error)?,
            )
            .map_err(|_| mcp::McpErrorCode::InternalError)?
        }
        "mylist_update_category" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or(mcp::McpErrorCode::InvalidArgument)?;
            let expected_revision = params
                .get("expectedRevision")
                .and_then(Value::as_i64)
                .filter(|value| *value >= 1)
                .ok_or(mcp::McpErrorCode::InvalidArgument)?;
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let color_id = params
                .get("colorId")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or(mcp::McpErrorCode::InvalidArgument)?
                .to_string();
            serde_json::to_value(
                store
                    .mcp_update_category(
                        &meta.request_id,
                        UpdateCategoryInput {
                            id: id.to_string(),
                            name,
                            color_id,
                        },
                        expected_revision,
                    )
                    .map_err(write_error)?,
            )
            .map_err(|_| mcp::McpErrorCode::InternalError)?
        }
        "mylist_prepare_delete_category" => {
            let id = params
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or(mcp::McpErrorCode::InvalidArgument)?;
            serde_json::to_value(store.mcp_prepare_delete_category(id).map_err(write_error)?)
                .map_err(|_| mcp::McpErrorCode::InternalError)?
        }
        "mylist_prepare_delete_task" => {
            let id = required_string(params, "id")?;
            serde_json::to_value(store.mcp_prepare_delete_task(id).map_err(write_error)?)
                .map_err(|_| mcp::McpErrorCode::InternalError)?
        }
        "mylist_delete_task" => {
            let id = required_string(params, "id")?;
            let expected_revision = required_revision(params)?;
            let token = required_string(params, "confirmationToken")?;
            let fingerprint = delete_fingerprint("delete_task", id, expected_revision, None);
            let confirmations = app.state::<mcp_confirmation::McpConfirmationState>();
            confirmations.validate(token, "delete_task", &fingerprint)?;
            let result = store
                .mcp_delete_task(&meta.request_id, id, expected_revision)
                .map_err(write_error)?;
            confirmations.consume(token);
            serde_json::to_value(result).map_err(|_| mcp::McpErrorCode::InternalError)?
        }
        "mylist_delete_category" => {
            let id = required_string(params, "id")?;
            let expected_revision = required_revision(params)?;
            let token = required_string(params, "confirmationToken")?;
            let target_category_id = params
                .get("targetCategoryId")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty());
            let fingerprint =
                delete_fingerprint("delete_category", id, expected_revision, target_category_id);
            let confirmations = app.state::<mcp_confirmation::McpConfirmationState>();
            confirmations.validate(token, "delete_category", &fingerprint)?;
            let result = store
                .mcp_delete_category(&meta.request_id, id, expected_revision, target_category_id)
                .map_err(write_error)?;
            confirmations.consume(token);
            serde_json::to_value(result).map_err(|_| mcp::McpErrorCode::InternalError)?
        }
        _ => return Err(mcp::McpErrorCode::NotFound),
    };
    if !matches!(
        name,
        "mylist_prepare_delete_category" | "mylist_prepare_delete_task"
    ) {
        let _ = app.emit(
            "mylist-data-changed",
            json!({"source": "mcp", "tool": name}),
        );
    }
    Ok(result)
}

fn available_tools() -> Value {
    json!([
        {"name": "mylist_get_overview", "description": "Read MyLIST service health, counts and capability metadata.", "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}},
        {"name": "mylist_list_tasks", "description": "Read a paged list of tasks.", "inputSchema": {"type": "object", "properties": {"status": {"type": "string", "enum": ["todo", "completed"]}, "categoryId": {"type": "string"}, "page": {"type": "integer", "minimum": 0}, "pageSize": {"type": "integer", "minimum": 1, "maximum": 100}}, "additionalProperties": false}},
        {"name": "mylist_get_task", "description": "Read one task by UUID.", "inputSchema": {"type": "object", "required": ["id"], "properties": {"id": {"type": "string"}}, "additionalProperties": false}},
        {"name": "mylist_list_categories", "description": "Read a paged list of categories and task counts.", "inputSchema": {"type": "object", "properties": {"page": {"type": "integer", "minimum": 0}, "pageSize": {"type": "integer", "minimum": 1, "maximum": 100}}, "additionalProperties": false}},
        {"name": "mylist_get_palette", "description": "Read the 24-color palette.", "inputSchema": {"type": "object", "properties": {"page": {"type": "integer", "minimum": 0}, "pageSize": {"type": "integer", "minimum": 1, "maximum": 100}}, "additionalProperties": false}},
        {"name": "mylist_get_operation", "description": "Read the redacted status of a locally approved import or export operation.", "inputSchema": {"type": "object", "required": ["operationId"], "properties": {"operationId": {"type": "string"}}, "additionalProperties": false}},
        {"name": "mylist_create_task", "description": "Create one to-do task. requestId makes retries idempotent. recurrence requires dueAtUtcMs.", "inputSchema": {"type": "object", "required": ["requestId", "protocolVersion", "title", "categoryId"], "properties": {"requestId": {"type": "string"}, "protocolVersion": {"type": "string"}, "title": {"type": "string"}, "note": {"type": "string"}, "categoryId": {"type": "string"}, "dueAtUtcMs": {"type": ["integer", "null"]}, "recurrence": {"type": ["object", "null"], "properties": {"interval": {"type": "integer", "minimum": 1, "maximum": 999}, "unit": {"type": "string", "enum": ["day", "week", "month", "year"]}, "action": {"type": "string", "enum": ["update_due", "create_new"]}, "baseTitle": {"type": "string"}}, "required": ["interval", "unit", "action"], "additionalProperties": false}}, "additionalProperties": false}},
        {"name": "mylist_update_task", "description": "Edit a task using its current revision. Omit recurrence to preserve it; use null to disable it. recurrence requires dueAtUtcMs.", "inputSchema": {"type": "object", "required": ["requestId", "protocolVersion", "id", "expectedRevision", "title", "categoryId"], "properties": {"requestId": {"type": "string"}, "protocolVersion": {"type": "string"}, "id": {"type": "string"}, "expectedRevision": {"type": "integer", "minimum": 1}, "title": {"type": "string"}, "note": {"type": "string"}, "categoryId": {"type": "string"}, "dueAtUtcMs": {"type": ["integer", "null"]}, "recurrence": {"type": ["object", "null"], "properties": {"interval": {"type": "integer", "minimum": 1, "maximum": 999}, "unit": {"type": "string", "enum": ["day", "week", "month", "year"]}, "action": {"type": "string", "enum": ["update_due", "create_new"]}, "baseTitle": {"type": "string"}}, "required": ["interval", "unit", "action"], "additionalProperties": false}}, "additionalProperties": false}},
        {"name": "mylist_complete_task", "description": "Move a task to completed using its current revision.", "inputSchema": {"type": "object", "required": ["requestId", "protocolVersion", "id", "expectedRevision"], "properties": {"requestId": {"type": "string"}, "protocolVersion": {"type": "string"}, "id": {"type": "string"}, "expectedRevision": {"type": "integer", "minimum": 1}}, "additionalProperties": false}},
        {"name": "mylist_restore_task", "description": "Move a completed task back to to-do using its current revision.", "inputSchema": {"type": "object", "required": ["requestId", "protocolVersion", "id", "expectedRevision"], "properties": {"requestId": {"type": "string"}, "protocolVersion": {"type": "string"}, "id": {"type": "string"}, "expectedRevision": {"type": "integer", "minimum": 1}}, "additionalProperties": false}},
        {"name": "mylist_create_category", "description": "Create a category. An omitted colorId receives the next available palette color.", "inputSchema": {"type": "object", "required": ["requestId", "protocolVersion", "name"], "properties": {"requestId": {"type": "string"}, "protocolVersion": {"type": "string"}, "name": {"type": "string"}, "colorId": {"type": "string"}}, "additionalProperties": false}},
        {"name": "mylist_update_category", "description": "Rename or recolor a category using its current revision.", "inputSchema": {"type": "object", "required": ["requestId", "protocolVersion", "id", "expectedRevision", "name", "colorId"], "properties": {"requestId": {"type": "string"}, "protocolVersion": {"type": "string"}, "id": {"type": "string"}, "expectedRevision": {"type": "integer", "minimum": 1}, "name": {"type": "string"}, "colorId": {"type": "string"}}, "additionalProperties": false}},
        {"name": "mylist_prepare_delete_task", "description": "Preview a task before requesting a local delete confirmation. This does not delete anything.", "inputSchema": {"type": "object", "required": ["requestId", "protocolVersion", "id"], "properties": {"requestId": {"type": "string"}, "protocolVersion": {"type": "string"}, "id": {"type": "string"}}, "additionalProperties": false}},
        {"name": "mylist_prepare_delete_category", "description": "Preview affected tasks and valid migration targets. This does not delete anything.", "inputSchema": {"type": "object", "required": ["requestId", "protocolVersion", "id"], "properties": {"requestId": {"type": "string"}, "protocolVersion": {"type": "string"}, "id": {"type": "string"}}, "additionalProperties": false}},
        {"name": "mylist_request_confirmation", "description": "Request visible local approval before deleting a task or category.", "inputSchema": {"type": "object", "required": ["requestId", "protocolVersion", "operation", "id", "expectedRevision"], "properties": {"requestId": {"type": "string"}, "protocolVersion": {"type": "string"}, "operation": {"type": "string", "enum": ["delete_task", "delete_category"]}, "id": {"type": "string"}, "expectedRevision": {"type": "integer", "minimum": 1}, "targetCategoryId": {"type": "string"}}, "additionalProperties": false}},
        {"name": "mylist_delete_task", "description": "Delete a task after matching local confirmation.", "inputSchema": {"type": "object", "required": ["requestId", "protocolVersion", "id", "expectedRevision", "confirmationToken"], "properties": {"requestId": {"type": "string"}, "protocolVersion": {"type": "string"}, "id": {"type": "string"}, "expectedRevision": {"type": "integer", "minimum": 1}, "confirmationToken": {"type": "string"}}, "additionalProperties": false}},
        {"name": "mylist_delete_category", "description": "Delete a category after matching local confirmation.", "inputSchema": {"type": "object", "required": ["requestId", "protocolVersion", "id", "expectedRevision", "confirmationToken"], "properties": {"requestId": {"type": "string"}, "protocolVersion": {"type": "string"}, "id": {"type": "string"}, "expectedRevision": {"type": "integer", "minimum": 1}, "targetCategoryId": {"type": "string"}, "confirmationToken": {"type": "string"}}, "additionalProperties": false}},
        {"name": "mylist_export_prepare", "description": "Request a local MyLIST export. The user chooses the destination and, for encrypted data, enters the password only in MyLIST.", "inputSchema": {"type": "object", "required": ["requestId", "protocolVersion", "format"], "properties": {"requestId": {"type": "string"}, "protocolVersion": {"type": "string"}, "format": {"type": "string", "enum": ["plaintext", "encrypted"]}}, "additionalProperties": false}},
        {"name": "mylist_export_confirm", "description": "Read the local export result after the user finishes the MyLIST confirmation flow.", "inputSchema": {"type": "object", "required": ["operationId"], "properties": {"operationId": {"type": "string"}}, "additionalProperties": false}},
        {"name": "mylist_import_prepare", "description": "Request a local MyLIST import. The user selects the source file and confirms the preview in MyLIST.", "inputSchema": {"type": "object", "required": ["requestId", "protocolVersion", "operation"], "properties": {"requestId": {"type": "string"}, "protocolVersion": {"type": "string"}, "operation": {"type": "string", "enum": ["merge", "replace"]}}, "additionalProperties": false}},
        {"name": "mylist_import_confirm", "description": "Read the local import result after the user finishes the MyLIST confirmation flow.", "inputSchema": {"type": "object", "required": ["operationId"], "properties": {"operationId": {"type": "string"}}, "additionalProperties": false}}
    ])
}

pub fn handle_request(app: Option<&AppHandle>, encoded: &str) -> String {
    // Some Windows JSON-RPC clients may prepend a UTF-8 BOM on their first
    // pipe write. Treat it as transport framing, not protocol content.
    let encoded = encoded.trim().trim_start_matches('\u{feff}');
    let parsed = serde_json::from_str::<JsonRpcRequest>(encoded);
    let request = match parsed {
        Ok(request) if request.jsonrpc.is_empty() || request.jsonrpc == "2.0" => request,
        Ok(request) => {
            return serde_json::to_string(&protocol_error(request.id, -32600, "Invalid Request"))
                .unwrap_or_default()
        }
        Err(_) => {
            return serde_json::to_string(&protocol_error(None, -32700, "Parse error"))
                .unwrap_or_default()
        }
    };
    let id = request.id.clone();
    if request.method == "notifications/initialized" {
        return String::new();
    }
    let response = match request.method.as_str() {
        "initialize" => success(
            id,
            json!({
                "protocolVersion": mcp::MCP_PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
            }),
        ),
        "tools/list" => success(id, json!({"tools": available_tools()})),
        "tools/call" => {
            let name = request.params.get("name").and_then(Value::as_str);
            let arguments = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let Some(name) = name else {
                return serde_json::to_string(&tool_error(id, mcp::McpErrorCode::InvalidArgument))
                    .unwrap_or_default();
            };
            let arguments = if arguments.is_object() {
                arguments
            } else {
                json!({})
            };
            match app {
                None => tool_error(id, mcp::McpErrorCode::ServiceUnavailable),
                Some(handle) => match if name == "mylist_request_confirmation" {
                    request_confirmation(handle, &arguments)
                } else if matches!(name, "mylist_export_prepare" | "mylist_import_prepare") {
                    transfer_prepare(handle, name, &arguments)
                } else if matches!(
                    name,
                    "mylist_get_operation" | "mylist_export_confirm" | "mylist_import_confirm"
                ) {
                    transfer_status(handle, &arguments)
                } else if name.starts_with("mylist_create_")
                    || matches!(
                        name,
                        "mylist_update_task"
                            | "mylist_complete_task"
                            | "mylist_restore_task"
                            | "mylist_update_category"
                            | "mylist_prepare_delete_category"
                            | "mylist_prepare_delete_task"
                            | "mylist_delete_task"
                            | "mylist_delete_category"
                    )
                {
                    write_tool(handle, name, &arguments)
                } else {
                    read_tool(handle, name, &arguments)
                } {
                    Ok(result) => success(
                        id,
                        json!({"content": [{"type": "text", "text": result.to_string()}], "structuredContent": result}),
                    ),
                    Err(code) => tool_error(id, code),
                },
            }
        }
        _ => protocol_error(id, -32601, "Method not found"),
    };
    serde_json::to_string(&response).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{available_tools, handle_request, page_params};
    use serde_json::json;

    #[test]
    fn initialize_exposes_read_capability() {
        let response: serde_json::Value = serde_json::from_str(&handle_request(
            None,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        ))
        .unwrap();
        assert_eq!(response["result"]["capabilities"]["tools"], json!({}));
    }

    #[test]
    fn initialized_notification_has_no_response() {
        assert!(handle_request(
            None,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#,
        )
        .is_empty());
    }

    #[test]
    fn tools_list_contains_destructive_confirmation_tools() {
        let tools = available_tools().as_array().unwrap().to_vec();
        assert_eq!(tools.len(), 21);
        assert!(tools
            .iter()
            .all(|tool| { tool["name"].as_str().unwrap().starts_with("mylist_") }));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "mylist_create_task"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "mylist_prepare_delete_category"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "mylist_request_confirmation"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "mylist_import_prepare"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "mylist_get_operation"));
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "mylist_delete_task"));
    }

    #[test]
    fn page_size_is_bounded() {
        assert_eq!(page_params(&json!({})).unwrap(), (0, 20));
        assert!(page_params(&json!({"pageSize": 0})).is_err());
        assert!(page_params(&json!({"pageSize": 101})).is_err());
    }

    #[test]
    fn disabled_service_returns_stable_error() {
        let response: serde_json::Value = serde_json::from_str(&handle_request(
            None,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"mylist_get_overview","arguments":{}}}"#,
        ))
        .unwrap();
        assert_eq!(
            response["result"]["structuredContent"]["code"],
            "MCP_SERVICE_UNAVAILABLE"
        );
    }
}
