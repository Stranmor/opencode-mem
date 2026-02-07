mod infinite;
mod knowledge;
mod memory;

use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_infinite::InfiniteMemory;
use opencode_mem_service::{ObservationService, SessionService};
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

#[expect(clippy::too_many_arguments, reason = "MCP handler needs all service references")]
pub fn handle_tool_call(
    storage: &Storage,
    embeddings: Option<&EmbeddingService>,
    infinite_mem: Option<&InfiniteMemory>,
    _observation_service: &ObservationService,
    _session_service: &SessionService,
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
        McpTool::Search => memory::handle_search(storage, &args),
        McpTool::Timeline => memory::handle_timeline(storage, &args),
        McpTool::GetObservations => memory::handle_get_observations(storage, &args),
        McpTool::MemoryGet => memory::handle_memory_get(storage, &args),
        McpTool::MemoryRecent => memory::handle_memory_recent(storage, &args),
        McpTool::MemoryHybridSearch => memory::handle_hybrid_search(storage, &args),
        McpTool::MemorySemanticSearch => memory::handle_semantic_search(storage, embeddings, &args),
        McpTool::KnowledgeSearch => knowledge::handle_knowledge_search(storage, &args),
        McpTool::KnowledgeSave => {
            return knowledge::handle_knowledge_save(storage, &args, id);
        },
        McpTool::KnowledgeGet => knowledge::handle_knowledge_get(storage, &args),
        McpTool::KnowledgeList => knowledge::handle_knowledge_list(storage, &args),
        McpTool::InfiniteExpand => {
            return infinite::handle_infinite_expand(infinite_mem, handle, &args, id);
        },
        McpTool::InfiniteTimeRange => {
            return infinite::handle_infinite_time_range(infinite_mem, handle, &args, id);
        },
        McpTool::InfiniteDrillHour => {
            return infinite::handle_infinite_drill_hour(infinite_mem, handle, &args, id);
        },
        McpTool::InfiniteDrillDay => {
            return infinite::handle_infinite_drill_day(infinite_mem, handle, &args, id);
        },
    };

    McpResponse { jsonrpc: "2.0".to_owned(), id, result: Some(result), error: None }
}
