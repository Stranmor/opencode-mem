mod infinite;
mod knowledge;
mod memory;

use opencode_mem_service::{
    InfiniteMemoryService, KnowledgeService, ObservationService, PendingWrite, PendingWriteQueue,
    SearchService, ServiceError, SessionService,
};
use serde::Serialize;
use serde_json::json;
use std::fmt::Display;
use std::sync::Arc;
use tokio::runtime::Handle;

use opencode_mem_core::{DEFAULT_QUERY_LIMIT, cap_query_limit};

use crate::tools::{McpTool, WORKFLOW_DOCS};
use crate::{McpError, McpResponse};

/// Parse a `limit` argument from MCP tool arguments.
///
/// Returns `default` when absent or non-numeric, clamped to `MAX_QUERY_LIMIT`.
/// Each caller passes the default matching its tool's JSON schema description.
/// Uses `usize::try_from` to avoid truncating `as` casts.
pub(crate) fn parse_limit(args: &serde_json::Value, default: usize) -> usize {
    let raw = args
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(u64::try_from(default).unwrap_or(u64::MAX));
    let uncapped = usize::try_from(raw).unwrap_or(usize::MAX);
    cap_query_limit(uncapped)
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

#[expect(
    clippy::too_many_arguments,
    reason = "MCP handler needs all service references"
)]
pub async fn handle_tool_call(
    infinite_mem: Option<&InfiniteMemoryService>,
    observation_service: &Arc<ObservationService>,
    _session_service: &Arc<SessionService>,
    knowledge_service: &Arc<KnowledgeService>,
    search_service: &Arc<SearchService>,
    pending_writes: &Arc<PendingWriteQueue>,
    handle: &Handle,
    params: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    let pre_cb_state = search_service.circuit_breaker().state_name();
    let tool_name_str = match params
        .get("name")
        .and_then(|n| n.as_str())
        .filter(|s| !s.is_empty())
    {
        Some(name) => name,
        None => {
            return McpResponse {
                jsonrpc: "2.0".to_owned(),
                id,
                result: None,
                error: Some(McpError {
                    code: -32602,
                    message: "Tool name is required and must not be empty".to_owned(),
                }),
            };
        }
    };
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let tool = match McpTool::parse(tool_name_str) {
        Some(t) => t,
        None => {
            return McpResponse {
                jsonrpc: "2.0".to_owned(),
                id,
                result: None,
                error: Some(McpError {
                    code: -32602,
                    message: format!(
                        "Unknown tool: '{tool_name_str}'. Available: __IMPORTANT, search, timeline, get_observations, memory_get, memory_recent, memory_hybrid_search, memory_semantic_search, save_memory, memory_delete, knowledge_search, knowledge_save, knowledge_get, knowledge_list, knowledge_delete, infinite_expand, infinite_time_range, infinite_drill_hour, infinite_drill_minute"
                    ),
                }),
            };
        }
    };

    let result = match tool {
        McpTool::Important => {
            json!({ "content": [{ "type": "text", "text": WORKFLOW_DOCS }] })
        }
        McpTool::Search => {
            memory::handle_search(search_service, &args, parse_limit(&args, 50)).await
        }
        McpTool::Timeline => {
            memory::handle_timeline(search_service, &args, parse_limit(&args, 50)).await
        }
        McpTool::GetObservations => memory::handle_get_observations(search_service, &args).await,
        McpTool::MemoryGet => memory::handle_memory_get(search_service, &args).await,
        McpTool::MemoryRecent => {
            memory::handle_memory_recent(search_service, &args, parse_limit(&args, 10)).await
        }
        McpTool::MemoryHybridSearch => {
            memory::handle_hybrid_search(
                search_service,
                &args,
                parse_limit(&args, DEFAULT_QUERY_LIMIT),
            )
            .await
        }
        McpTool::MemorySemanticSearch => {
            memory::handle_semantic_search(
                search_service,
                &args,
                parse_limit(&args, DEFAULT_QUERY_LIMIT),
            )
            .await
        }
        McpTool::SaveMemory => {
            memory::handle_save_memory(observation_service, pending_writes, &args).await
        }
        McpTool::MemoryDelete => memory::handle_memory_delete(observation_service, &args).await,
        McpTool::KnowledgeSearch => {
            knowledge::handle_knowledge_search(knowledge_service, &args, parse_limit(&args, 10))
                .await
        }
        McpTool::KnowledgeSave => knowledge::handle_knowledge_save(knowledge_service, &args).await,
        McpTool::KnowledgeGet => knowledge::handle_knowledge_get(knowledge_service, &args).await,
        McpTool::KnowledgeList => {
            knowledge::handle_knowledge_list(
                knowledge_service,
                &args,
                parse_limit(&args, DEFAULT_QUERY_LIMIT),
            )
            .await
        }
        McpTool::KnowledgeDelete => {
            knowledge::handle_knowledge_delete(knowledge_service, &args).await
        }
        McpTool::InfiniteExpand => {
            return infinite::handle_infinite_expand(infinite_mem, handle, &args, id).await;
        }
        McpTool::InfiniteTimeRange => {
            return infinite::handle_infinite_time_range(infinite_mem, handle, &args, id).await;
        }
        McpTool::InfiniteDrillHour => {
            return infinite::handle_infinite_drill_hour(infinite_mem, handle, &args, id).await;
        }
        McpTool::InfiniteDrillMinute => {
            return infinite::handle_infinite_drill_minute(infinite_mem, handle, &args, id).await;
        }
    };

    if pre_cb_state != "closed" && search_service.circuit_breaker().state_name() == "closed" {
        search_service.handle_recovery();
        spawn_pending_flush(observation_service, pending_writes);
    }

    McpResponse {
        jsonrpc: "2.0".to_owned(),
        id,
        result: Some(result),
        error: None,
    }
}

fn spawn_pending_flush(
    observation_service: &Arc<ObservationService>,
    pending_writes: &Arc<PendingWriteQueue>,
) {
    if pending_writes.is_empty() {
        return;
    }
    if !pending_writes.start_flush() {
        return;
    }

    let observation_service = Arc::clone(observation_service);
    let pending_writes = Arc::clone(pending_writes);
    tokio::spawn(async move {
        let pending_count = pending_writes.len();
        tracing::info!(
            pending_count,
            "Flushing pending save_memory writes after DB recovery"
        );

        loop {
            let Some(item) = pending_writes.pop_front() else {
                break;
            };

            let PendingWrite::SaveMemory {
                text,
                title,
                project,
                observation_type,
                noise_level,
            } = item;
            match observation_service
                .save_memory(
                    &text,
                    title.as_deref(),
                    project.as_deref(),
                    observation_type,
                    noise_level,
                )
                .await
            {
                Ok(_) => {}
                Err(e) if e.is_db_unavailable() || e.is_transient() => {
                    pending_writes.push_front(PendingWrite::SaveMemory {
                        text,
                        title,
                        project,
                        observation_type,
                        noise_level,
                    });
                    tracing::warn!(
                        error = %e,
                        remaining = pending_writes.len(),
                        "Pending write flush paused: database became unavailable again"
                    );
                    break;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Pending save_memory flush dropped one invalid item");
                }
            }
        }

        pending_writes.finish_flush();
    });
}
