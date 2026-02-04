use opencode_mem_embeddings::{EmbeddingProvider, EmbeddingService};
use opencode_mem_storage::Storage;
use serde::Serialize;
use serde_json::json;
use std::fmt::Display;

use crate::tools::{McpTool, WORKFLOW_DOCS};
use crate::{McpError, McpResponse};

pub(crate) fn mcp_ok<T: Serialize>(data: &T) -> serde_json::Value {
    match serde_json::to_string_pretty(data) {
        Ok(json) => json!({ "content": [{ "type": "text", "text": json }] }),
        Err(e) => {
            json!({ "content": [{ "type": "text", "text": format!("Serialization error: {}", e) }], "isError": true })
        }
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
    params: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    let tool_name_str = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let tool = match McpTool::parse(tool_name_str) {
        Some(t) => t,
        None => {
            return McpResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(McpError {
                    code: -32602,
                    message: format!("Unknown tool: '{}'. Available: __IMPORTANT, search, timeline, get_observations, memory_get, memory_recent, memory_hybrid_search, memory_semantic_search, knowledge_search, knowledge_save, knowledge_get, knowledge_list, infinite_expand, infinite_time_range, infinite_drill_hour, infinite_drill_day", tool_name_str),
                }),
            };
        }
    };

    let result = match tool {
        McpTool::Important => {
            json!({ "content": [{ "type": "text", "text": WORKFLOW_DOCS }] })
        }
        McpTool::Search => {
            let query = args.get("query").and_then(|q| q.as_str());
            let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(50) as usize;
            let project = args.get("project").and_then(|p| p.as_str());
            let obs_type = args.get("type").and_then(|t| t.as_str());
            match storage.search_with_filters(query, project, obs_type, limit) {
                Ok(results) => mcp_ok(&results),
                Err(e) => mcp_err(e),
            }
        }
        McpTool::Timeline => {
            let from = args.get("from").and_then(|f| f.as_str());
            let to = args.get("to").and_then(|t| t.as_str());
            let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(50) as usize;
            match storage.get_timeline(from, to, limit) {
                Ok(results) => mcp_ok(&results),
                Err(e) => mcp_err(e),
            }
        }
        McpTool::GetObservations => {
            let ids: Vec<String> = args
                .get("ids")
                .and_then(|i| i.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            if ids.is_empty() {
                mcp_err("ids array is required and must not be empty")
            } else {
                match storage.get_observations_by_ids(&ids) {
                    Ok(results) => mcp_ok(&results),
                    Err(e) => mcp_err(e),
                }
            }
        }
        McpTool::MemoryGet => {
            let id_str = args.get("id").and_then(|i| i.as_str()).unwrap_or("");
            match storage.get_by_id(id_str) {
                Ok(Some(obs)) => mcp_ok(&obs),
                Ok(None) => mcp_text(&format!("Observation not found: {}", id_str)),
                Err(e) => mcp_err(e),
            }
        }
        McpTool::MemoryRecent => {
            let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(10) as usize;
            match storage.get_recent(limit) {
                Ok(results) => mcp_ok(&results),
                Err(e) => mcp_err(e),
            }
        }
        McpTool::MemoryHybridSearch => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(20) as usize;
            match storage.hybrid_search(query, limit) {
                Ok(results) => mcp_ok(&results),
                Err(e) => mcp_err(e),
            }
        }
        McpTool::MemorySemanticSearch => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(20) as usize;

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
                            }
                            Err(e) => mcp_err(e),
                        },
                        Err(e) => {
                            tracing::warn!("Failed to embed query, falling back to hybrid: {}", e);
                            match storage.hybrid_search(query, limit) {
                                Ok(results) => mcp_ok(&results),
                                Err(e) => mcp_err(e),
                            }
                        }
                    }
                }
                None => {
                    // No embeddings service, use hybrid search
                    match storage.hybrid_search(query, limit) {
                        Ok(results) => mcp_ok(&results),
                        Err(e) => mcp_err(e),
                    }
                }
            }
        }
        McpTool::KnowledgeSearch => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(10) as usize;
            match storage.search_knowledge(query, limit) {
                Ok(results) => mcp_ok(&results),
                Err(e) => mcp_err(e),
            }
        }
        McpTool::KnowledgeSave => {
            return handle_knowledge_save(storage, &args, id);
        }
        McpTool::KnowledgeGet => {
            let id_str = args.get("id").and_then(|i| i.as_str()).unwrap_or("");
            match storage.get_knowledge(id_str) {
                Ok(Some(knowledge)) => mcp_ok(&knowledge),
                Ok(None) => mcp_text(&format!("Knowledge not found: {}", id_str)),
                Err(e) => mcp_err(e),
            }
        }
        McpTool::KnowledgeList => {
            let knowledge_type = args
                .get("knowledge_type")
                .and_then(|t| t.as_str())
                .and_then(|s| s.parse::<opencode_mem_core::KnowledgeType>().ok());
            let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(20) as usize;
            match storage.list_knowledge(knowledge_type, limit) {
                Ok(results) => mcp_ok(&results),
                Err(e) => mcp_err(e),
            }
        }
        McpTool::InfiniteExpand => {
            mcp_err("Infinite Memory not connected to MCP server. Use HTTP API at /api/infinite/expand_summary/:id")
        }
        McpTool::InfiniteTimeRange => {
            mcp_err("Infinite Memory not connected to MCP server. Use HTTP API at /api/infinite/time_range")
        }
        McpTool::InfiniteDrillHour => {
            mcp_err("Infinite Memory not connected to MCP server. Use HTTP API at /api/infinite/drill_hour/:id")
        }
        McpTool::InfiniteDrillDay => {
            mcp_err("Infinite Memory not connected to MCP server. Use HTTP API at /api/infinite/drill_day/:id")
        }
    };

    McpResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
        error: None,
    }
}

fn handle_knowledge_save(
    storage: &Storage,
    args: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    let knowledge_type_str = args
        .get("knowledge_type")
        .and_then(|t| t.as_str())
        .unwrap_or("skill");
    let knowledge_type = match knowledge_type_str.parse::<opencode_mem_core::KnowledgeType>() {
        Ok(kt) => kt,
        Err(e) => {
            return McpResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(mcp_err(format!("Invalid knowledge_type: {}", e))),
                error: None,
            };
        }
    };
    let title = match args.get("title").and_then(|t| t.as_str()) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => {
            return McpResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(mcp_err("title is required and cannot be empty")),
                error: None,
            };
        }
    };
    let description = match args.get("description").and_then(|d| d.as_str()) {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => {
            return McpResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(mcp_err("description is required and cannot be empty")),
                error: None,
            };
        }
    };
    let instructions = args
        .get("instructions")
        .and_then(|i| i.as_str())
        .map(|s| s.to_string());
    let triggers: Vec<String> = args
        .get("triggers")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let source_project = args
        .get("source_project")
        .and_then(|p| p.as_str())
        .map(|s| s.to_string());
    let source_observation = args
        .get("source_observation")
        .and_then(|o| o.as_str())
        .map(|s| s.to_string());

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
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(mcp_ok(&knowledge)),
            error: None,
        },
        Err(e) => McpResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(mcp_err(e)),
            error: None,
        },
    }
}
