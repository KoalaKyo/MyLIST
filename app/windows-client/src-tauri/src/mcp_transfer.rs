//! Local-only operation state for MCP initiated import and export flows.
//!
//! An MCP client can request an operation, but it never receives a local path
//! or an encryption password. The desktop UI performs file selection and the
//! final confirmation, then the MCP client polls the redacted operation state.

use std::{collections::HashMap, sync::Mutex};

use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::mcp::McpErrorCode;

const TRANSFER_TTL_MS: i64 = 10 * 60_000;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTransferDto {
    pub operation_id: String,
    pub operation: String,
    pub status: String,
    pub expires_at_utc_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
}

#[derive(Clone, Debug)]
struct PendingTransfer {
    operation: String,
    status: String,
    expires_at_utc_ms: i64,
    preview: Option<Value>,
    result: Option<Value>,
}

#[derive(Default)]
pub struct McpTransferState {
    pending: Mutex<HashMap<String, PendingTransfer>>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

impl McpTransferState {
    pub fn request(&self, operation: &str) -> Result<McpTransferDto, McpErrorCode> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| McpErrorCode::InternalError)?;
        let now = now_ms();
        pending.retain(|_, item| item.expires_at_utc_ms > now);
        let operation_id = Uuid::new_v4().to_string();
        let item = PendingTransfer {
            operation: operation.to_string(),
            status: "awaiting_local_action".to_string(),
            expires_at_utc_ms: now + TRANSFER_TTL_MS,
            preview: None,
            result: None,
        };
        let dto = dto(&operation_id, &item);
        pending.insert(operation_id, item);
        Ok(dto)
    }

    pub fn get(&self, operation_id: &str) -> Result<McpTransferDto, McpErrorCode> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| McpErrorCode::InternalError)?;
        let now = now_ms();
        pending.retain(|_, item| item.expires_at_utc_ms > now);
        let item = pending.get(operation_id).ok_or(McpErrorCode::NotFound)?;
        Ok(dto(operation_id, item))
    }

    pub fn assert_operation(&self, operation_id: &str, expected: &str) -> Result<(), McpErrorCode> {
        let dto = self.get(operation_id)?;
        if dto.operation != expected || dto.status != "awaiting_local_action" {
            return Err(McpErrorCode::OperationRejected);
        }
        Ok(())
    }

    pub fn set_preview(&self, operation_id: &str, preview: Value) -> Result<(), McpErrorCode> {
        self.update(operation_id, "awaiting_confirmation", Some(preview), None)
    }

    pub fn complete(&self, operation_id: &str, result: Value) -> Result<(), McpErrorCode> {
        self.update(operation_id, "completed", None, Some(result))
    }

    pub fn cancel(&self, operation_id: &str) -> Result<(), McpErrorCode> {
        let current = self.get(operation_id)?;
        if !current.status.starts_with("awaiting_") {
            return Err(McpErrorCode::OperationRejected);
        }
        self.update(operation_id, "cancelled", None, None)
    }

    pub fn fail(&self, operation_id: &str, code: &str) -> Result<(), McpErrorCode> {
        self.update(
            operation_id,
            "failed",
            None,
            Some(serde_json::json!({"code": code})),
        )
    }

    fn update(
        &self,
        operation_id: &str,
        status: &str,
        preview: Option<Value>,
        result: Option<Value>,
    ) -> Result<(), McpErrorCode> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| McpErrorCode::InternalError)?;
        let now = now_ms();
        let item = pending
            .get_mut(operation_id)
            .ok_or(McpErrorCode::NotFound)?;
        if item.expires_at_utc_ms <= now {
            pending.remove(operation_id);
            return Err(McpErrorCode::NotFound);
        }
        item.status = status.to_string();
        if preview.is_some() {
            item.preview = preview;
        }
        if result.is_some() {
            item.result = result;
        }
        Ok(())
    }
}

fn dto(operation_id: &str, item: &PendingTransfer) -> McpTransferDto {
    McpTransferDto {
        operation_id: operation_id.to_string(),
        operation: item.operation.clone(),
        status: item.status.clone(),
        expires_at_utc_ms: item.expires_at_utc_ms,
        preview: item.preview.clone(),
        result: item.result.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_is_local_and_redacts_passwords_and_paths() {
        let state = McpTransferState::default();
        let pending = state.request("export_encrypted").unwrap();
        assert_eq!(pending.status, "awaiting_local_action");
        state
            .complete(
                &pending.operation_id,
                serde_json::json!({"fileName":"backup.dtodo"}),
            )
            .unwrap();
        let result = state.get(&pending.operation_id).unwrap();
        assert_eq!(result.status, "completed");
        assert!(result.result.unwrap().get("fileName").is_some());
    }
}
