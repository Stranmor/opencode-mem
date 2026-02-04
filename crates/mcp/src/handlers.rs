use opencode_mem_embeddings::{EmbeddingProvider, EmbeddingService};
use opencode_mem_infinite::InfiniteMemory;
use opencode_mem_storage::Storage;
use serde::Serialize;
use serde_json::json;
use std::fmt::Display;
use tokio::runtime::Handle;

use crate::tools::{McpTool, WORKFLOW_DOCS};
use crate::{McpError, McpResponse};

pub(crate) fn mcp_ok<T: Serialize>(data: &T) -> serde_json::Value {
    match serde_json::to_string_pretty(data) {
        Ok(json) => json!({ "content": [{ "type": "text", "text": json }] }),
        Err(e) => {
            json!({ "content": [{ "type": "text", "text": format!("Serialization error: {}", e) }], "isError": true })
        },
    }
}

pub(crate) fn mcp_text(text: &str) -> serde_json::Value {
    json!({ "content": [{ "type": "text", "text": text }] })
}

pub(crate) fn mcp_err(msg: impl Display) -> serde_json::Value {
    json!({ "content": [{ "type": "text", "text": format!("Error: {}", msg) }], "isError": true })
}

pub fn handle_tool_call(
    storage: &Storage,
    embeddings: Option<&EmbeddingService>,
    infinite_mem: Option<&InfiniteMemory>,
    handle: &Handle,
    params: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    let tool_name_str = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
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
                    message: format!("Unknown tool: '{tool_name_str}'. Available: __IMPORTANT, search, timeline, get_observations, memory_get, memory_recent, memory_hybrid_search, memory_semantic_search, knowledge_search, knowledge_save, knowledge_get, knowledge_list, infinite_expand, infinite_time_range, infinite_drill_hour, infinite_drill_day"),
                }),
            };
        },
    };

    let result = match tool {
        McpTool::Important => {
            json!({ "content": [{ "type": "text", "text": WORKFLOW_DOCS }] })
        },
        McpTool::Search => {
            let query = args.get("query").and_then(|q| q.as_str());
            let limit =
                args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(50) as usize;
            let project = args.get("project").and_then(|p| p.as_str());
            let obs_type = args.get("type").and_then(|t| t.as_str());
            match storage.search_with_filters(query, project, obs_type, limit) {
                Ok(results) => mcp_ok(&results),
                Err(e) => mcp_err(e),
            }
        },
        McpTool::Timeline => {
            let from = args.get("from").and_then(|f| f.as_str());
            let to = args.get("to").and_then(|t| t.as_str());
            let limit =
                args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(50) as usize;
            match storage.get_timeline(from, to, limit) {
                Ok(results) => mcp_ok(&results),
                Err(e) => mcp_err(e),
            }
        },
        McpTool::GetObservations => {
            let ids: Vec<String> = args
                .get("ids")
                .and_then(|i| i.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(ToOwned::to_owned)).collect())
                .unwrap_or_default();
            if ids.is_empty() {
                mcp_err("ids array is required and must not be empty")
            } else {
                match storage.get_observations_by_ids(&ids) {
                    Ok(results) => mcp_ok(&results),
                    Err(e) => mcp_err(e),
                }
            }
        },
        McpTool::MemoryGet => {
            let id_str = args.get("id").and_then(|i| i.as_str()).unwrap_or("");
            match storage.get_by_id(id_str) {
                Ok(Some(obs)) => mcp_ok(&obs),
                Ok(None) => mcp_text(&format!("Observation not found: {id_str}")),
                Err(e) => mcp_err(e),
            }
        },
        McpTool::MemoryRecent => {
            let limit =
                args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(10) as usize;
            match storage.get_recent(limit) {
                Ok(results) => mcp_ok(&results),
                Err(e) => mcp_err(e),
            }
        },
        McpTool::MemoryHybridSearch => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let limit =
                args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(20) as usize;
            match storage.hybrid_search(query, limit) {
                Ok(results) => mcp_ok(&results),
                Err(e) => mcp_err(e),
            }
        },
        McpTool::MemorySemanticSearch => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let limit =
                args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(20) as usize;

            match embeddings {
                Some(emb) => {
                    match emb.embed(query) {
                        Ok(query_vec) => match storage.semantic_search(&query_vec, limit) {
                            Ok(results) if !results.is_empty() => mcp_ok(&results),
                            Ok(_) => {
                                // No vector results, fallback to hybrid
                                match storage.hybrid_search(query, limit) {
                                    Ok(results) => mcp_ok(&results),
                                    Err(e) => mcp_err(e),
                                }
                            },
                            Err(e) => mcp_err(e),
                        },
                        Err(e) => {
                            tracing::warn!("Failed to embed query, falling back to hybrid: {}", e);
                            match storage.hybrid_search(query, limit) {
                                Ok(results) => mcp_ok(&results),
                                Err(e) => mcp_err(e),
                            }
                        },
                    }
                },
                None => {
                    // No embeddings service, use hybrid search
                    match storage.hybrid_search(query, limit) {
                        Ok(results) => mcp_ok(&results),
                        Err(e) => mcp_err(e),
                    }
                },
            }
        },
        McpTool::KnowledgeSearch => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let limit =
                args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(10) as usize;
            match storage.search_knowledge(query, limit) {
                Ok(results) => mcp_ok(&results),
                Err(e) => mcp_err(e),
            }
        },
        McpTool::KnowledgeSave => {
            return handle_knowledge_save(storage, &args, id);
        },
        McpTool::KnowledgeGet => {
            let id_str = args.get("id").and_then(|i| i.as_str()).unwrap_or("");
            match storage.get_knowledge(id_str) {
                Ok(Some(knowledge)) => mcp_ok(&knowledge),
                Ok(None) => mcp_text(&format!("Knowledge not found: {id_str}")),
                Err(e) => mcp_err(e),
            }
        },
        McpTool::KnowledgeList => {
            let knowledge_type = args
                .get("knowledge_type")
                .and_then(|t| t.as_str())
                .and_then(|s| s.parse::<opencode_mem_core::KnowledgeType>().ok());
            let limit =
                args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(20) as usize;
            match storage.list_knowledge(knowledge_type, limit) {
                Ok(results) => mcp_ok(&results),
                Err(e) => mcp_err(e),
            }
        },
        McpTool::InfiniteExpand => match infinite_mem {
            Some(mem) => {
                let summary_id =
                    args.get("summary_id").and_then(serde_json::Value::as_i64).unwrap_or(0);
                let limit = args.get("limit").and_then(serde_json::Value::as_i64).unwrap_or(1000);
                match handle.block_on(mem.get_events_by_summary_id(summary_id, limit)) {
                    Ok(events) => mcp_ok(&events),
                    Err(e) => mcp_err(e),
                }
            },
            None => mcp_err("Infinite Memory not configured (INFINITE_MEMORY_URL not set)"),
        },
        McpTool::InfiniteTimeRange => match infinite_mem {
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
                match handle.block_on(mem.get_events_by_time_range(start, end, session_id, limit)) {
                    Ok(events) => mcp_ok(&events),
                    Err(e) => mcp_err(e),
                }
            },
            None => mcp_err("Infinite Memory not configured (INFINITE_MEMORY_URL not set)"),
        },
        McpTool::InfiniteDrillHour => match infinite_mem {
            Some(mem) => {
                let hour_id = args.get("hour_id").and_then(serde_json::Value::as_i64).unwrap_or(0);
                let limit = args.get("limit").and_then(serde_json::Value::as_i64).unwrap_or(100);
                match handle.block_on(mem.get_5min_summaries_by_hour_id(hour_id, limit)) {
                    Ok(summaries) => mcp_ok(&summaries),
                    Err(e) => mcp_err(e),
                }
            },
            None => mcp_err("Infinite Memory not configured (INFINITE_MEMORY_URL not set)"),
        },
        McpTool::InfiniteDrillDay => match infinite_mem {
            Some(mem) => {
                let day_id = args.get("day_id").and_then(serde_json::Value::as_i64).unwrap_or(0);
                let limit = args.get("limit").and_then(serde_json::Value::as_i64).unwrap_or(100);
                match handle.block_on(mem.get_hour_summaries_by_day_id(day_id, limit)) {
                    Ok(summaries) => mcp_ok(&summaries),
                    Err(e) => mcp_err(e),
                }
            },
            None => mcp_err("Infinite Memory not configured (INFINITE_MEMORY_URL not set)"),
        },
    };

    McpResponse { jsonrpc: "2.0".to_owned(), id, result: Some(result), error: None }
}

fn handle_knowledge_save(
    storage: &Storage,
    args: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    let knowledge_type_str = args.get("knowledge_type").and_then(|t| t.as_str()).unwrap_or("skill");
    let knowledge_type = match knowledge_type_str.parse::<opencode_mem_core::KnowledgeType>() {
        Ok(kt) => kt,
        Err(e) => {
            return McpResponse {
                jsonrpc: "2.0".to_owned(),
                id,
                result: Some(mcp_err(format!("Invalid knowledge_type: {e}"))),
                error: None,
            };
        },
    };
    let title = match args.get("title").and_then(|t| t.as_str()) {
        Some(t) if !t.is_empty() => t.to_owned(),
        _ => {
            return McpResponse {
                jsonrpc: "2.0".to_owned(),
                id,
                result: Some(mcp_err("title is required and cannot be empty")),
                error: None,
            };
        },
    };
    let description = match args.get("description").and_then(|d| d.as_str()) {
        Some(d) if !d.is_empty() => d.to_owned(),
        _ => {
            return McpResponse {
                jsonrpc: "2.0".to_owned(),
                id,
                result: Some(mcp_err("description is required and cannot be empty")),
                error: None,
            };
        },
    };
    let instructions = args.get("instructions").and_then(|i| i.as_str()).map(ToOwned::to_owned);
    let triggers: Vec<String> = args
        .get("triggers")
        .and_then(|t| t.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(ToOwned::to_owned)).collect())
        .unwrap_or_default();
    let source_project = args.get("source_project").and_then(|p| p.as_str()).map(ToOwned::to_owned);
    let source_observation =
        args.get("source_observation").and_then(|o| o.as_str()).map(ToOwned::to_owned);

    let input = opencode_mem_core::KnowledgeInput {
        knowledge_type,
        title,
        description,
        instructions,
        triggers,
        source_project,
        source_observation,
    };

    match storage.save_knowledge(input) {
        Ok(knowledge) => McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(mcp_ok(&knowledge)),
            error: None,
        },
        Err(e) => {
            McpResponse { jsonrpc: "2.0".to_owned(), id, result: Some(mcp_err(e)), error: None }
        },
    }
}
