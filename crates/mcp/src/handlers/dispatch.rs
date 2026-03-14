use std::sync::Arc;

use opencode_mem_core::DEFAULT_QUERY_LIMIT;
use opencode_mem_service::{
    InfiniteMemoryService, KnowledgeService, ObservationService, PendingWriteQueue, SearchService,
    SessionService,
};
use serde_json::json;
use tokio::runtime::Handle;

use crate::tools::{McpTool, WORKFLOW_DOCS};
use crate::{McpError, McpResponse};

use super::{infinite, knowledge, memory, parse_limit};

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
                        "Unknown tool: '{tool_name_str}'. Available: {}",
                        McpTool::all_tool_names().join(", ")
                    ),
                }),
            };
        }
    };

    let result = dispatch_tool(
        tool,
        &args,
        infinite_mem,
        observation_service,
        knowledge_service,
        search_service,
        pending_writes,
        handle,
        id.clone(),
    )
    .await;

    // Early return for infinite memory tools (they return full McpResponse)
    let result = match result {
        DispatchResult::Json(v) => v,
        DispatchResult::FullResponse(r) => return r,
    };

    if pre_cb_state != "closed" && search_service.circuit_breaker().state_name() == "closed" {
        search_service.handle_recovery();
        opencode_mem_service::spawn_pending_flush(
            observation_service,
            knowledge_service,
            pending_writes,
        );
    }

    McpResponse {
        jsonrpc: "2.0".to_owned(),
        id,
        result: Some(result),
        error: None,
    }
}

enum DispatchResult {
    Json(serde_json::Value),
    FullResponse(McpResponse),
}

#[expect(
    clippy::too_many_arguments,
    reason = "Dispatch needs all service references for routing"
)]
async fn dispatch_tool(
    tool: McpTool,
    args: &serde_json::Value,
    infinite_mem: Option<&InfiniteMemoryService>,
    observation_service: &Arc<ObservationService>,
    knowledge_service: &Arc<KnowledgeService>,
    search_service: &Arc<SearchService>,
    pending_writes: &Arc<PendingWriteQueue>,
    handle: &Handle,
    id: serde_json::Value,
) -> DispatchResult {
    match tool {
        McpTool::Important => {
            DispatchResult::Json(json!({ "content": [{ "type": "text", "text": WORKFLOW_DOCS }] }))
        }
        McpTool::Search => DispatchResult::Json(
            memory::handle_search(search_service, args, parse_limit(args, DEFAULT_QUERY_LIMIT))
                .await,
        ),
        McpTool::Timeline => DispatchResult::Json(
            memory::handle_timeline(search_service, args, parse_limit(args, DEFAULT_QUERY_LIMIT))
                .await,
        ),
        McpTool::GetObservations => {
            DispatchResult::Json(memory::handle_get_observations(search_service, args).await)
        }
        McpTool::MemoryGet => {
            DispatchResult::Json(memory::handle_memory_get(search_service, args).await)
        }
        McpTool::MemoryRecent => DispatchResult::Json(
            memory::handle_memory_recent(search_service, args, parse_limit(args, 10)).await,
        ),
        McpTool::MemoryHybridSearch => DispatchResult::Json(
            memory::handle_hybrid_search(
                search_service,
                args,
                parse_limit(args, DEFAULT_QUERY_LIMIT),
            )
            .await,
        ),
        McpTool::MemorySemanticSearch => DispatchResult::Json(
            memory::handle_semantic_search(
                search_service,
                args,
                parse_limit(args, DEFAULT_QUERY_LIMIT),
            )
            .await,
        ),
        McpTool::SaveMemory => DispatchResult::Json(
            memory::handle_save_memory(observation_service, pending_writes, args).await,
        ),
        McpTool::MemoryDelete => DispatchResult::Json(
            memory::handle_memory_delete(observation_service, pending_writes, args).await,
        ),
        McpTool::KnowledgeSearch => DispatchResult::Json(
            knowledge::handle_knowledge_search(knowledge_service, args, parse_limit(args, 10))
                .await,
        ),
        McpTool::KnowledgeSave => DispatchResult::Json(
            knowledge::handle_knowledge_save(knowledge_service, pending_writes, args).await,
        ),
        McpTool::KnowledgeGet => {
            DispatchResult::Json(knowledge::handle_knowledge_get(knowledge_service, args).await)
        }
        McpTool::KnowledgeList => DispatchResult::Json(
            knowledge::handle_knowledge_list(
                knowledge_service,
                args,
                parse_limit(args, DEFAULT_QUERY_LIMIT),
            )
            .await,
        ),
        McpTool::KnowledgeDelete => DispatchResult::Json(
            knowledge::handle_knowledge_delete(knowledge_service, pending_writes, args).await,
        ),
        McpTool::InfiniteExpand => DispatchResult::FullResponse(
            infinite::handle_infinite_expand(infinite_mem, handle, args, id).await,
        ),
        McpTool::InfiniteTimeRange => DispatchResult::FullResponse(
            infinite::handle_infinite_time_range(infinite_mem, handle, args, id).await,
        ),
        McpTool::InfiniteDrillDay => DispatchResult::FullResponse(
            infinite::handle_infinite_drill_day(infinite_mem, handle, args, id).await,
        ),
        McpTool::InfiniteDrillHour => DispatchResult::FullResponse(
            infinite::handle_infinite_drill_hour(infinite_mem, handle, args, id).await,
        ),
        McpTool::InfiniteSearchEntities => DispatchResult::FullResponse(
            infinite::handle_infinite_search_entities(infinite_mem, handle, args, id).await,
        ),
    }
}
