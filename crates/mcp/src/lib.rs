//! MCP (Model Context Protocol) server for opencode-mem.

#![allow(missing_docs, reason = "Internal crate with self-explanatory API")]
#![allow(clippy::as_conversions, reason = "u64 to usize conversions are safe")]
#![allow(clippy::cast_possible_truncation, reason = "Sizes are within bounds")]
#![allow(clippy::option_if_let_else, reason = "if let is clearer")]
#![allow(clippy::needless_pass_by_value, reason = "API design choice")]
#![allow(
    clippy::let_underscore_must_use,
    reason = "Intentionally ignoring results"
)]
#![allow(let_underscore_drop, reason = "Intentionally dropping values")]
#![allow(unreachable_pub, reason = "pub items are re-exported")]
#![allow(clippy::redundant_pub_crate, reason = "Explicit visibility")]
#![allow(unused_results, reason = "Some results are intentionally ignored")]
#![allow(missing_debug_implementations, reason = "Internal types")]
#![allow(clippy::if_then_some_else_none, reason = "Style preference")]
#![allow(clippy::let_underscore_untyped, reason = "Type is clear from context")]
#![allow(clippy::absolute_paths, reason = "Explicit paths for clarity")]
#![allow(clippy::pattern_type_mismatch, reason = "Pattern matching style")]
#![allow(clippy::too_many_lines, reason = "Handler functions are complex")]
#![allow(clippy::manual_let_else, reason = "if let is clearer")]
#![allow(clippy::or_fun_call, reason = "unwrap_or with function is acceptable")]
#![allow(clippy::missing_docs_in_private_items, reason = "Internal crate")]
#![allow(clippy::implicit_return, reason = "Implicit return is idiomatic Rust")]
#![allow(clippy::question_mark_used, reason = "? operator is idiomatic Rust")]
#![allow(clippy::min_ident_chars, reason = "Short error vars are idiomatic")]
#![allow(
    clippy::shadow_unrelated,
    reason = "Shadowing in match arms is idiomatic"
)]
#![allow(clippy::shadow_reuse, reason = "Shadowing for unwrapping is idiomatic")]
#![allow(clippy::exhaustive_enums, reason = "MCP tools are stable")]
#![allow(clippy::exhaustive_structs, reason = "MCP types are stable")]
#![allow(
    clippy::single_call_fn,
    reason = "Handler functions improve readability"
)]

pub mod handlers;
mod tools;

use opencode_mem_service::{
    InfiniteMemoryService, KnowledgeService, ObservationService, PendingWriteQueue, SearchService,
    SessionService,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::runtime::Handle;

pub use tools::McpTool;

pub use handlers::handle_tool_call;
use tools::get_tools_json;

#[derive(Deserialize)]
struct McpRequest {
    #[expect(dead_code, reason = "Required by JSON-RPC protocol but not used")]
    jsonrpc: String,
    id: Option<serde_json::Value>,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Serialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

#[derive(Serialize, Debug)]
pub struct McpError {
    pub code: i32,
    pub message: String,
}

pub async fn run_mcp_server(
    infinite_mem: Option<Arc<InfiniteMemoryService>>,
    observation_service: Arc<ObservationService>,
    session_service: Arc<SessionService>,
    knowledge_service: Arc<KnowledgeService>,
    search_service: Arc<SearchService>,
    handle: Handle,
) {
    tracing::info!("MCP server starting on stdio");
    let pending_writes = Arc::new(PendingWriteQueue::new());
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin).lines();

    while let Ok(Some(line)) = reader.next_line().await {
        if line.is_empty() {
            continue;
        }

        let json_value: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let error_response = McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id: json!(null),
                    result: None,
                    error: Some(McpError {
                        code: -32700,
                        message: format!("Parse error: {e}"),
                    }),
                };
                if let Ok(json) = serde_json::to_string(&error_response) {
                    if let Err(e) = stdout.write_all(format!("{json}\n").as_bytes()).await {
                        tracing::error!("MCP stdout write error on parse error response: {}", e);
                        break;
                    }
                    if let Err(e) = stdout.flush().await {
                        tracing::error!("MCP stdout flush error on parse error response: {}", e);
                        break;
                    }
                }
                continue;
            }
        };

        let request: McpRequest = match serde_json::from_value(json_value.clone()) {
            Ok(r) => r,
            Err(e) => {
                let error_response = McpResponse {
                    jsonrpc: "2.0".to_owned(),
                    id: json_value.get("id").cloned().unwrap_or(json!(null)),
                    result: None,
                    error: Some(McpError {
                        code: -32600,
                        message: format!("Invalid Request: {e}"),
                    }),
                };
                if let Ok(json) = serde_json::to_string(&error_response) {
                    if let Err(e) = stdout.write_all(format!("{json}\n").as_bytes()).await {
                        tracing::error!(
                            "MCP stdout write error on invalid request response: {}",
                            e
                        );
                        break;
                    }
                    if let Err(e) = stdout.flush().await {
                        tracing::error!(
                            "MCP stdout flush error on invalid request response: {}",
                            e
                        );
                        break;
                    }
                }
                continue;
            }
        };

        if let Some(response) = handle_request(
            infinite_mem.as_deref(),
            &observation_service,
            &session_service,
            &knowledge_service,
            &search_service,
            &pending_writes,
            &handle,
            &request,
        )
        .await
            && let Ok(response_json) = serde_json::to_string(&response)
        {
            if let Err(e) = stdout
                .write_all(format!("{response_json}\n").as_bytes())
                .await
            {
                tracing::error!("MCP stdout write error: {}", e);
                break;
            }
            if let Err(e) = stdout.flush().await {
                tracing::error!("MCP stdout flush error: {}", e);
                break;
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "MCP request dispatch needs shared service refs"
)]
async fn handle_request(
    infinite_mem: Option<&InfiniteMemoryService>,
    observation_service: &Arc<ObservationService>,
    session_service: &Arc<SessionService>,
    knowledge_service: &Arc<KnowledgeService>,
    search_service: &Arc<SearchService>,
    pending_writes: &Arc<PendingWriteQueue>,
    handle: &Handle,
    req: &McpRequest,
) -> Option<McpResponse> {
    let id = match &req.id {
        Some(id) => id.clone(),
        None => return None,
    };

    Some(match req.method.as_str() {
        "initialize" => McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "opencode-memory", "version": "0.1.0" }
            })),
            error: None,
        },
        "tools/list" => McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(get_tools_json()),
            error: None,
        },
        "tools/call" => {
            handle_tool_call(
                infinite_mem,
                observation_service,
                session_service,
                knowledge_service,
                search_service,
                pending_writes,
                handle,
                &req.params,
                id,
            )
            .await
        }
        _ => McpResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result: None,
            error: Some(McpError {
                code: -32601,
                message: format!("Method not found: {}", req.method),
            }),
        },
    })
}

#[cfg(test)]
mod tests;
