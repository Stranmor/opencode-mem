use opencode_mem_infinite::InfiniteMemory;
use tokio::runtime::Handle;

use super::{mcp_err, mcp_ok};
use crate::McpResponse;

pub(super) async fn handle_infinite_expand(
    infinite_mem: Option<&InfiniteMemory>,
    _handle: &Handle,
    args: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    match infinite_mem {
        Some(mem) => {
            let summary_id =
                args.get("summary_id").and_then(serde_json::Value::as_i64).unwrap_or(0);
            let limit = args.get("limit").and_then(serde_json::Value::as_i64).unwrap_or(1000);
            match mem.get_events_by_summary_id(summary_id, limit).await {
                Ok(events) => McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_ok(&events)),
                    error: None,
                },
                Err(e) => McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_err(e)),
                    error: None,
                },
            }
        },
        None => McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(mcp_err("Infinite Memory not configured (INFINITE_MEMORY_URL not set)")),
            error: None,
        },
    }
}

pub(super) async fn handle_infinite_time_range(
    infinite_mem: Option<&InfiniteMemory>,
    _handle: &Handle,
    args: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    match infinite_mem {
        Some(mem) => {
            let from = args.get("from").and_then(|f| f.as_str()).unwrap_or("");
            let to = args.get("to").and_then(|t| t.as_str()).unwrap_or("");
            let session_id = args.get("session_id").and_then(|s| s.as_str());
            let limit = args.get("limit").and_then(serde_json::Value::as_i64).unwrap_or(1000);
            let start = match chrono::DateTime::parse_from_rfc3339(from) {
                Ok(dt) => dt.with_timezone(&chrono::Utc),
                Err(_) => {
                    return McpResponse {
                        jsonrpc: "2.0".to_owned(),
                        id,
                        result: Some(mcp_err("Invalid 'from' datetime format (use RFC3339)")),
                        error: None,
                    }
                },
            };
            let end = match chrono::DateTime::parse_from_rfc3339(to) {
                Ok(dt) => dt.with_timezone(&chrono::Utc),
                Err(_) => {
                    return McpResponse {
                        jsonrpc: "2.0".to_owned(),
                        id,
                        result: Some(mcp_err("Invalid 'to' datetime format (use RFC3339)")),
                        error: None,
                    }
                },
            };
            match mem.get_events_by_time_range(start, end, session_id, limit).await {
                Ok(events) => McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_ok(&events)),
                    error: None,
                },
                Err(e) => McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_err(e)),
                    error: None,
                },
            }
        },
        None => McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(mcp_err("Infinite Memory not configured (INFINITE_MEMORY_URL not set)")),
            error: None,
        },
    }
}

pub(super) async fn handle_infinite_drill_hour(
    infinite_mem: Option<&InfiniteMemory>,
    _handle: &Handle,
    args: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    match infinite_mem {
        Some(mem) => {
            let day_id = args.get("id").and_then(serde_json::Value::as_i64).unwrap_or(0);
            let limit = args.get("limit").and_then(serde_json::Value::as_i64).unwrap_or(100);
            match mem.get_hour_summaries_by_day_id(day_id, limit).await {
                Ok(summaries) => McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_ok(&summaries)),
                    error: None,
                },
                Err(e) => McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_err(e)),
                    error: None,
                },
            }
        },
        None => McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(mcp_err("Infinite Memory not configured (INFINITE_MEMORY_URL not set)")),
            error: None,
        },
    }
}

pub(super) async fn handle_infinite_drill_minute(
    infinite_mem: Option<&InfiniteMemory>,
    _handle: &Handle,
    args: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    match infinite_mem {
        Some(mem) => {
            let hour_id = args.get("id").and_then(serde_json::Value::as_i64).unwrap_or(0);
            let limit = args.get("limit").and_then(serde_json::Value::as_i64).unwrap_or(100);
            match mem.get_5min_summaries_by_hour_id(hour_id, limit).await {
                Ok(summaries) => McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_ok(&summaries)),
                    error: None,
                },
                Err(e) => McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id,
                    result: Some(mcp_err(e)),
                    error: None,
                },
            }
        },
        None => McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(mcp_err("Infinite Memory not configured (INFINITE_MEMORY_URL not set)")),
            error: None,
        },
    }
}
