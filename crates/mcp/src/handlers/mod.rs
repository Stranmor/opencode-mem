mod dispatch;
mod infinite;
mod knowledge;
mod memory;

pub use dispatch::handle_tool_call;

use opencode_mem_service::ServiceError;
use serde::Serialize;
use serde_json::json;
use std::fmt::Display;

/// Parse a `limit` argument from MCP tool arguments.
///
/// Returns `default` when absent or non-numeric.
/// Each caller passes the default matching its tool's JSON schema description.
/// Uses `usize::try_from` to avoid truncating `as` casts.
pub(crate) fn parse_limit(args: &serde_json::Value, default: usize) -> usize {
    let raw = args
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(u64::try_from(default).unwrap_or(u64::MAX));
    let limit = usize::try_from(raw).unwrap_or(usize::MAX);
    opencode_mem_core::cap_query_limit(limit)
}

pub(crate) fn mcp_ok<T: Serialize>(data: &T) -> serde_json::Value {
    match serde_json::to_string_pretty(data) {
        Ok(json) => json!({ "content": [{ "type": "text", "text": json }] }),
        Err(e) => {
            json!({ "content": [{ "type": "text", "text": format!("Serialization error: {}", e) }], "isError": true })
        }
    }
}

#[allow(dead_code, reason = "Used in tests and available for future handlers")]
pub(crate) fn mcp_text(text: &str) -> serde_json::Value {
    json!({ "content": [{ "type": "text", "text": text }] })
}

pub(crate) fn mcp_err(msg: impl Display) -> serde_json::Value {
    json!({ "content": [{ "type": "text", "text": format!("Error: {}", msg) }], "isError": true })
}

/// Fast-fail check for MCP **read** handlers when the circuit breaker is open.
///
/// Returns `Some(empty_json_response)` if the CB is open (database unavailable),
/// allowing the handler to return immediately without waiting for a 3s pool timeout.
/// Returns `None` if the CB allows the request through (circuit closed or half-open probe).
pub(crate) fn cb_fast_fail_read<T: Serialize + Default>(
    cb: &opencode_mem_storage::CircuitBreaker,
) -> Option<serde_json::Value> {
    if !cb.should_allow() {
        tracing::debug!(
            "MCP read: circuit breaker blocking request, fast-failing with empty results"
        );
        Some(mcp_ok(&T::default()))
    } else {
        None
    }
}

/// Fast-fail check for MCP **write** handlers when the circuit breaker is open.
///
/// Returns `Some(degraded_json_response)` if the CB is open, allowing the handler
/// to return immediately. Returns `None` if the CB allows the request through.
pub(crate) fn cb_fast_fail_write(
    cb: &opencode_mem_storage::CircuitBreaker,
) -> Option<serde_json::Value> {
    if !cb.should_allow() {
        tracing::debug!(
            "MCP write: circuit breaker blocking request, fast-failing with degraded response"
        );
        Some(mcp_ok(&json!({ "success": false, "degraded": true })))
    } else {
        None
    }
}

/// Handle a service error for **read** operations with graceful degradation.
///
/// When the database is unavailable (circuit breaker open or connection failure),
/// returns an empty result set instead of an error — preventing IDE injection errors.
pub(crate) fn degrade_read_err<T: Serialize + Default>(
    err: ServiceError,
    cb: &opencode_mem_storage::CircuitBreaker,
) -> serde_json::Value {
    if err.is_db_unavailable() || err.is_transient() {
        cb.record_failure();
        tracing::warn!(error = %err, "MCP read: database unavailable, returning empty results");
        mcp_ok(&T::default())
    } else {
        mcp_err(err)
    }
}

/// Handle a service error for **write** operations with graceful degradation.
///
/// When the database is unavailable, silently skips the write and returns
/// a valid JSON object instead of failing. Returns `{"success": false, "degraded": true}`
/// so the IDE plugin can parse it without crashing (plugin expects JSON, not plain text).
pub(crate) fn degrade_write_err(
    err: ServiceError,
    cb: &opencode_mem_storage::CircuitBreaker,
) -> serde_json::Value {
    if err.is_db_unavailable() || err.is_transient() {
        cb.record_failure();
        tracing::warn!(error = %err, "MCP write: database unavailable, skipping write");
        mcp_ok(&json!({ "success": false, "degraded": true }))
    } else {
        mcp_err(err)
    }
}
