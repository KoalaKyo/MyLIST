//! Short-lived, local-only confirmation tokens for destructive MCP actions.
//!
//! Tokens never reach SQLite or export files.  They exist only while the
//! desktop process is alive and are approved or rejected by a visible local
//! MyLIST window.

use std::{collections::HashMap, sync::Mutex};

use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::mcp::McpErrorCode;

const TOKEN_TTL_MS: i64 = 60_000;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmationRequestDto {
    pub token: String,
    pub operation: String,
    pub expires_at_utc_ms: i64,
}

#[derive(Clone, Debug)]
struct PendingConfirmation {
    operation: String,
    fingerprint: String,
    scope: String,
    expires_at_utc_ms: i64,
    approved: bool,
}

#[derive(Default)]
pub struct McpConfirmationState {
    pending: Mutex<HashMap<String, PendingConfirmation>>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// A deterministic FNV-1a fingerprint is sufficient here: it only binds an
/// in-memory, one-minute token to the exact JSON parameters in this process.
pub fn fingerprint(value: &Value) -> String {
    let encoded = serde_json::to_string(value).unwrap_or_default();
    let hash = encoded
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    format!("{hash:016x}")
}

impl McpConfirmationState {
    pub fn request(
        &self,
        operation: &str,
        scope: String,
        fingerprint: String,
    ) -> Result<ConfirmationRequestDto, McpErrorCode> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| McpErrorCode::InternalError)?;
        let now = now_ms();
        pending.retain(|_, item| item.expires_at_utc_ms > now);
        // There can be only one outstanding destructive request for one item.
        pending.retain(|_, item| item.scope != scope);
        let token = Uuid::new_v4().to_string();
        let expires_at_utc_ms = now + TOKEN_TTL_MS;
        pending.insert(
            token.clone(),
            PendingConfirmation {
                operation: operation.to_string(),
                fingerprint,
                scope,
                expires_at_utc_ms,
                approved: false,
            },
        );
        Ok(ConfirmationRequestDto {
            token,
            operation: operation.to_string(),
            expires_at_utc_ms,
        })
    }

    pub fn approve(&self, token: &str) -> Result<(), McpErrorCode> {
        self.set_approval(token, true)
    }

    pub fn reject(&self, token: &str) -> Result<(), McpErrorCode> {
        self.set_approval(token, false)
    }

    fn set_approval(&self, token: &str, approved: bool) -> Result<(), McpErrorCode> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| McpErrorCode::InternalError)?;
        let now = now_ms();
        let Some(item) = pending.get_mut(token) else {
            return Err(McpErrorCode::ConfirmationExpired);
        };
        if item.expires_at_utc_ms <= now {
            pending.remove(token);
            return Err(McpErrorCode::ConfirmationExpired);
        }
        if approved {
            item.approved = true;
        } else {
            pending.remove(token);
        }
        Ok(())
    }

    /// Checks an approved token but keeps it until the database transaction has
    /// succeeded. This lets a revision conflict be re-confirmed instead of
    /// silently consuming the user's approval.
    pub fn validate(
        &self,
        token: &str,
        operation: &str,
        fingerprint: &str,
    ) -> Result<(), McpErrorCode> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| McpErrorCode::InternalError)?;
        let now = now_ms();
        let Some(item) = pending.get(token) else {
            return Err(McpErrorCode::ConfirmationExpired);
        };
        if item.expires_at_utc_ms <= now {
            pending.remove(token);
            return Err(McpErrorCode::ConfirmationExpired);
        }
        if item.operation != operation || item.fingerprint != fingerprint {
            return Err(McpErrorCode::OperationRejected);
        }
        if !item.approved {
            return Err(McpErrorCode::ConfirmationRequired);
        }
        Ok(())
    }

    pub fn consume(&self, token: &str) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn token_requires_local_approval_and_exact_parameters() {
        let state = McpConfirmationState::default();
        let parameters = json!({"id":"task-a","expectedRevision":2});
        let token = state
            .request(
                "delete_task",
                "task:task-a".into(),
                fingerprint(&parameters),
            )
            .unwrap();
        assert_eq!(
            state.validate(&token.token, "delete_task", &fingerprint(&parameters)),
            Err(McpErrorCode::ConfirmationRequired)
        );
        state.approve(&token.token).unwrap();
        assert!(state
            .validate(&token.token, "delete_task", &fingerprint(&parameters))
            .is_ok());
        assert_eq!(
            state.validate(
                &token.token,
                "delete_task",
                &fingerprint(&json!({"id":"task-a","expectedRevision":3}))
            ),
            Err(McpErrorCode::OperationRejected)
        );
        state.consume(&token.token);
        assert_eq!(
            state.validate(&token.token, "delete_task", &fingerprint(&parameters)),
            Err(McpErrorCode::ConfirmationExpired)
        );
    }
}
