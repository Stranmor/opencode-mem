use opencode_mem_core::{INFINITE_MEMORY_NOT_CONFIGURED, MAX_QUERY_LIMIT_I64};
use opencode_mem_service::InfiniteMemoryService;
use tokio::runtime::Handle;

use super::{mcp_err, mcp_ok};
use crate::McpResponse;

fn degrade_infinite_read(
    err: opencode_mem_storage::StorageError,
    mem: &InfiniteMemoryService,
    id: serde_json::Value,
) -> McpResponse {
    let cb = mem.circuit_breaker();

    if err.is_unavailable() || err.is_transient() || cb.is_open() {
        if !cb.is_open() {
            cb.record_failure();
        }
        tracing::warn!(error = %err, "Infinite memory: database unavailable, returning empty results");
        McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(mcp_ok(&Vec::<serde_json::Value>::new())),
            error: None,
        }
    } else {
        McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(mcp_err(err)),
            error: None,
        }
    }
}

/// Fast-fail for infinite memory read handlers when the circuit breaker is open.
///
/// Returns `Some(empty_McpResponse)` if the CB blocks the request (database unavailable),
/// preventing a full connection-timeout hang on the single-threaded stdio MCP stream.
/// Returns `None` if the CB allows the request through (circuit closed or half-open probe).
fn cb_fast_fail_infinite(
    mem: &InfiniteMemoryService,
    id: &serde_json::Value,
) -> Option<McpResponse> {
    let cb = mem.circuit_breaker();
    if !cb.should_allow() {
        tracing::debug!(
            "MCP infinite read: circuit breaker blocking request, fast-failing with empty results"
        );
        Some(McpResponse {
            jsonrpc: "2.0".to_owned(),
            id: id.clone(),
            result: Some(mcp_ok(&Vec::<serde_json::Value>::new())),
            error: None,
        })
    } else {
        None
    }
}

pub(super) async fn handle_infinite_expand(
    infinite_mem: Option<&InfiniteMemoryService>,
    _handle: &Handle,
    args: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    match infinite_mem {
        Some(mem) => {
            if let Some(degraded) = cb_fast_fail_infinite(mem, &id) {
                return degraded;
            }
            let Some(summary_id) = args.get("id").and_then(serde_json::Value::as_i64) else {
                return McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_err("Missing or invalid 'id' (summary ID)")),
                    error: None,
                };
            };
            let limit = args
                .get("limit")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(1000)
                .clamp(1, MAX_QUERY_LIMIT_I64);
            match mem.get_events_by_summary_id(summary_id, limit).await {
                Ok(events) => McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_ok(&events)),
                    error: None,
                },
                Err(e) => degrade_infinite_read(e, mem, id),
            }
        }
        None => McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(mcp_err(INFINITE_MEMORY_NOT_CONFIGURED)),
            error: None,
        },
    }
}

pub(super) async fn handle_infinite_time_range(
    infinite_mem: Option<&InfiniteMemoryService>,
    _handle: &Handle,
    args: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    match infinite_mem {
        Some(mem) => {
            if let Some(degraded) = cb_fast_fail_infinite(mem, &id) {
                return degraded;
            }
            let from = args
                .get("start")
                .and_then(|f| f.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("");
            let to = args
                .get("end")
                .and_then(|t| t.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("");
            let session_id = args.get("session_id").and_then(|s| s.as_str());
            let limit = args
                .get("limit")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(1000)
                .clamp(1, MAX_QUERY_LIMIT_I64);
            let start = match chrono::DateTime::parse_from_rfc3339(from) {
                Ok(dt) => dt.with_timezone(&chrono::Utc),
                Err(_) => {
                    return McpResponse {
                        jsonrpc: "2.0".to_owned(),
                        id,
                        result: Some(mcp_err("Invalid 'start' datetime format (use RFC3339)")),
                        error: None,
                    };
                }
            };
            let end = match chrono::DateTime::parse_from_rfc3339(to) {
                Ok(dt) => dt.with_timezone(&chrono::Utc),
                Err(_) => {
                    return McpResponse {
                        jsonrpc: "2.0".to_owned(),
                        id,
                        result: Some(mcp_err("Invalid 'end' datetime format (use RFC3339)")),
                        error: None,
                    };
                }
            };
            match mem
                .get_events_by_time_range(start, end, session_id, limit)
                .await
            {
                Ok(events) => McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_ok(&events)),
                    error: None,
                },
                Err(e) => degrade_infinite_read(e, mem, id),
            }
        }
        None => McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(mcp_err(INFINITE_MEMORY_NOT_CONFIGURED)),
            error: None,
        },
    }
}

pub(super) async fn handle_infinite_drill_day(
    infinite_mem: Option<&InfiniteMemoryService>,
    _handle: &Handle,
    args: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    match infinite_mem {
        Some(mem) => {
            if let Some(degraded) = cb_fast_fail_infinite(mem, &id) {
                return degraded;
            }
            let Some(day_id) = args.get("id").and_then(serde_json::Value::as_i64) else {
                return McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_err("Missing or invalid 'id' (day summary ID)")),
                    error: None,
                };
            };
            let limit = args
                .get("limit")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(100)
                .clamp(1, MAX_QUERY_LIMIT_I64);
            match mem.get_hour_summaries_by_day_id(day_id, limit).await {
                Ok(summaries) => McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_ok(&summaries)),
                    error: None,
                },
                Err(e) => degrade_infinite_read(e, mem, id),
            }
        }
        None => McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(mcp_err(INFINITE_MEMORY_NOT_CONFIGURED)),
            error: None,
        },
    }
}

pub(super) async fn handle_infinite_drill_hour(
    infinite_mem: Option<&InfiniteMemoryService>,
    _handle: &Handle,
    args: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    match infinite_mem {
        Some(mem) => {
            if let Some(degraded) = cb_fast_fail_infinite(mem, &id) {
                return degraded;
            }
            let Some(hour_id) = args.get("id").and_then(serde_json::Value::as_i64) else {
                return McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_err("Missing or invalid 'id' (hour summary ID)")),
                    error: None,
                };
            };
            let limit = args
                .get("limit")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(100)
                .clamp(1, MAX_QUERY_LIMIT_I64);
            match mem.get_5min_summaries_by_hour_id(hour_id, limit).await {
                Ok(summaries) => McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_ok(&summaries)),
                    error: None,
                },
                Err(e) => degrade_infinite_read(e, mem, id),
            }
        }
        None => McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(mcp_err(INFINITE_MEMORY_NOT_CONFIGURED)),
            error: None,
        },
    }
}

pub(super) async fn handle_infinite_search_entities(
    infinite_mem: Option<&InfiniteMemoryService>,
    _handle: &Handle,
    args: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    match infinite_mem {
        Some(mem) => {
            if let Some(degraded) = cb_fast_fail_infinite(mem, &id) {
                return degraded;
            }
            let entity_type = args.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let value = args.get("value").and_then(|v| v.as_str()).unwrap_or("");
            let limit = args
                .get("limit")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(100)
                .clamp(1, MAX_QUERY_LIMIT_I64);

            if entity_type.is_empty() || value.is_empty() {
                return McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_err("Both 'type' and 'value' are required")),
                    error: None,
                };
            }

            match mem.search_by_entity(entity_type, value, limit).await {
                Ok(summaries) => McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_ok(&summaries)),
                    error: None,
                },
                Err(e) => degrade_infinite_read(e, mem, id),
            }
        }
        None => McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(mcp_err(INFINITE_MEMORY_NOT_CONFIGURED)),
            error: None,
        },
    }
}
