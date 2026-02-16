mod infinite;
mod knowledge;
mod memory;

use opencode_mem_infinite::InfiniteMemory;
use opencode_mem_service::{KnowledgeService, ObservationService, SearchService, SessionService};
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
pub async fn handle_tool_call(
    infinite_mem: Option<&InfiniteMemory>,
    observation_service: &ObservationService,
    _session_service: &SessionService,
    knowledge_service: &KnowledgeService,
    search_service: &SearchService,
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
                    message: format!("Unknown tool: '{tool_name_str}'. Available: __IMPORTANT, search, timeline, get_observations, memory_get, memory_recent, memory_hybrid_search, memory_semantic_search, save_memory, knowledge_search, knowledge_save, knowledge_get, knowledge_list, knowledge_delete, infinite_expand, infinite_time_range, infinite_drill_hour, infinite_drill_minute"),
                }),
            };
        },
    };

    let result = match tool {
        McpTool::Important => {
            json!({ "content": [{ "type": "text", "text": WORKFLOW_DOCS }] })
        },
        McpTool::Search => memory::handle_search(search_service, &args).await,
        McpTool::Timeline => memory::handle_timeline(search_service, &args).await,
        McpTool::GetObservations => memory::handle_get_observations(search_service, &args).await,
        McpTool::MemoryGet => memory::handle_memory_get(search_service, &args).await,
        McpTool::MemoryRecent => memory::handle_memory_recent(search_service, &args).await,
        McpTool::MemoryHybridSearch => memory::handle_hybrid_search(search_service, &args).await,
        McpTool::MemorySemanticSearch => {
            memory::handle_semantic_search(search_service, &args).await
        },
        McpTool::SaveMemory => memory::handle_save_memory(observation_service, &args).await,
        McpTool::KnowledgeSearch => {
            knowledge::handle_knowledge_search(knowledge_service, &args).await
        },
        McpTool::KnowledgeSave => knowledge::handle_knowledge_save(knowledge_service, &args).await,
        McpTool::KnowledgeGet => knowledge::handle_knowledge_get(knowledge_service, &args).await,
        McpTool::KnowledgeList => knowledge::handle_knowledge_list(knowledge_service, &args).await,
        McpTool::KnowledgeDelete => {
            knowledge::handle_knowledge_delete(knowledge_service, &args).await
        },
        McpTool::InfiniteExpand => {
            return infinite::handle_infinite_expand(infinite_mem, handle, &args, id).await;
        },
        McpTool::InfiniteTimeRange => {
            return infinite::handle_infinite_time_range(infinite_mem, handle, &args, id).await;
        },
        McpTool::InfiniteDrillHour => {
            return infinite::handle_infinite_drill_hour(infinite_mem, handle, &args, id).await;
        },
        McpTool::InfiniteDrillMinute => {
            return infinite::handle_infinite_drill_minute(infinite_mem, handle, &args, id).await;
        },
    };

    McpResponse { jsonrpc: "2.0".to_owned(), id, result: Some(result), error: None }
}
