//! MCP contract and security baseline.
//!
//! This module intentionally contains no listener, subprocess, database or
//! file-system code.  It is the single Rust-side definition of the public
//! contract that later MCP Bridge modules will implement.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MCP_PROTOCOL_VERSION: &str = "mylist.mcp.v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Read,
    Write,
    HighRisk,
}

impl RiskLevel {
    pub const fn requires_confirmation(self) -> bool {
        matches!(self, Self::HighRisk)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum McpErrorCode {
    ServiceDisabled,
    ServiceUnavailable,
    ProtocolMismatch,
    InvalidRequest,
    InvalidArgument,
    NotFound,
    Conflict,
    ConfirmationRequired,
    ConfirmationExpired,
    OperationRejected,
    RateLimited,
    InternalError,
}

impl McpErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ServiceDisabled => "MCP_SERVICE_DISABLED",
            Self::ServiceUnavailable => "MCP_SERVICE_UNAVAILABLE",
            Self::ProtocolMismatch => "MCP_PROTOCOL_MISMATCH",
            Self::InvalidRequest => "MCP_INVALID_REQUEST",
            Self::InvalidArgument => "MCP_INVALID_ARGUMENT",
            Self::NotFound => "MCP_NOT_FOUND",
            Self::Conflict => "MCP_CONFLICT",
            Self::ConfirmationRequired => "MCP_CONFIRMATION_REQUIRED",
            Self::ConfirmationExpired => "MCP_CONFIRMATION_EXPIRED",
            Self::OperationRejected => "MCP_OPERATION_REJECTED",
            Self::RateLimited => "MCP_RATE_LIMITED",
            Self::InternalError => "MCP_INTERNAL_ERROR",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDescriptor {
    pub name: String,
    pub risk: RiskLevel,
    pub requires_confirmation: bool,
    pub description_key: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpContract {
    pub protocol_version: String,
    pub tools: Vec<McpToolDescriptor>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRequestMeta {
    pub request_id: String,
    pub protocol_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpError {
    pub code: String,
    pub message_key: String,
    pub retryable: bool,
}

pub fn contract() -> McpContract {
    let read = |name: &str, key: &str| McpToolDescriptor {
        name: name.to_string(),
        risk: RiskLevel::Read,
        requires_confirmation: false,
        description_key: key.to_string(),
    };
    let write = |name: &str, key: &str| McpToolDescriptor {
        name: name.to_string(),
        risk: RiskLevel::Write,
        requires_confirmation: false,
        description_key: key.to_string(),
    };
    let high_risk = |name: &str, key: &str| McpToolDescriptor {
        name: name.to_string(),
        risk: RiskLevel::HighRisk,
        requires_confirmation: true,
        description_key: key.to_string(),
    };

    McpContract {
        protocol_version: MCP_PROTOCOL_VERSION.to_string(),
        tools: vec![
            read("mylist_get_overview", "mcp.tool.getOverview"),
            read("mylist_list_tasks", "mcp.tool.listTasks"),
            read("mylist_get_task", "mcp.tool.getTask"),
            read("mylist_list_categories", "mcp.tool.listCategories"),
            read("mylist_get_palette", "mcp.tool.getPalette"),
            read("mylist_get_operation", "mcp.tool.getOperation"),
            write("mylist_create_task", "mcp.tool.createTask"),
            write("mylist_update_task", "mcp.tool.updateTask"),
            write("mylist_complete_task", "mcp.tool.completeTask"),
            write("mylist_restore_task", "mcp.tool.restoreTask"),
            high_risk("mylist_delete_task", "mcp.tool.deleteTask"),
            write("mylist_create_category", "mcp.tool.createCategory"),
            write("mylist_update_category", "mcp.tool.updateCategory"),
            high_risk("mylist_delete_category", "mcp.tool.deleteCategory"),
            write(
                "mylist_restore_default_categories",
                "mcp.tool.restoreDefaultCategories",
            ),
            write("mylist_export_prepare", "mcp.tool.exportPrepare"),
            high_risk("mylist_export_confirm", "mcp.tool.exportConfirm"),
            write("mylist_import_prepare", "mcp.tool.importPrepare"),
            high_risk("mylist_import_confirm", "mcp.tool.importConfirm"),
        ],
    }
}

pub fn validate_request_meta(meta: &McpRequestMeta) -> Result<(), McpErrorCode> {
    let request_id = meta.request_id.trim();
    if request_id.is_empty() || request_id.len() > 128 {
        return Err(McpErrorCode::InvalidRequest);
    }
    if meta.protocol_version != MCP_PROTOCOL_VERSION {
        return Err(McpErrorCode::ProtocolMismatch);
    }
    Ok(())
}

pub fn error(code: McpErrorCode, retryable: bool) -> McpError {
    McpError {
        code: code.as_str().to_string(),
        message_key: format!("mcp.error.{}", code.as_str()),
        retryable,
    }
}

pub fn empty_params() -> Value {
    Value::Object(Default::default())
}

#[cfg(test)]
mod tests {
    use super::{
        contract, error, validate_request_meta, McpErrorCode, McpRequestMeta, RiskLevel,
        MCP_PROTOCOL_VERSION,
    };

    #[test]
    fn contract_has_unique_tool_names_and_stable_protocol() {
        let contract = contract();
        assert_eq!(contract.protocol_version, MCP_PROTOCOL_VERSION);
        let mut names = contract
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), contract.tools.len());
        assert!(contract
            .tools
            .iter()
            .any(|tool| { tool.name == "mylist_list_tasks" && tool.risk == RiskLevel::Read }));
        assert!(contract
            .tools
            .iter()
            .any(|tool| tool.name == "mylist_import_confirm" && tool.requires_confirmation));
    }

    #[test]
    fn contract_covers_each_documented_tool() {
        let names = contract()
            .tools
            .into_iter()
            .map(|tool| tool.name)
            .collect::<std::collections::HashSet<_>>();
        for expected in [
            "mylist_get_overview",
            "mylist_list_tasks",
            "mylist_get_task",
            "mylist_list_categories",
            "mylist_get_palette",
            "mylist_get_operation",
            "mylist_create_task",
            "mylist_update_task",
            "mylist_complete_task",
            "mylist_restore_task",
            "mylist_delete_task",
            "mylist_create_category",
            "mylist_update_category",
            "mylist_delete_category",
            "mylist_restore_default_categories",
            "mylist_export_prepare",
            "mylist_export_confirm",
            "mylist_import_prepare",
            "mylist_import_confirm",
        ] {
            assert!(names.contains(expected), "missing tool: {expected}");
        }
    }

    #[test]
    fn request_meta_rejects_bad_version_and_empty_ids() {
        let mut meta = McpRequestMeta {
            request_id: "req-1".into(),
            protocol_version: MCP_PROTOCOL_VERSION.into(),
        };
        assert!(validate_request_meta(&meta).is_ok());
        meta.request_id.clear();
        assert_eq!(
            validate_request_meta(&meta),
            Err(McpErrorCode::InvalidRequest)
        );
        meta.request_id = "req-2".into();
        meta.protocol_version = "unknown".into();
        assert_eq!(
            validate_request_meta(&meta),
            Err(McpErrorCode::ProtocolMismatch)
        );
    }

    #[test]
    fn errors_expose_stable_code_and_translation_key_only() {
        let value = error(McpErrorCode::ConfirmationRequired, false);
        assert_eq!(value.code, "MCP_CONFIRMATION_REQUIRED");
        assert_eq!(value.message_key, "mcp.error.MCP_CONFIRMATION_REQUIRED");
    }
}
