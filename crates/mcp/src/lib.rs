//! MCP (Model Context Protocol) server for opencode-mem.

#![allow(missing_docs, reason = "Internal crate with self-explanatory API")]
#![allow(clippy::as_conversions, reason = "u64 to usize conversions are safe")]
#![allow(clippy::cast_possible_truncation, reason = "Sizes are within bounds")]
#![allow(clippy::option_if_let_else, reason = "if let is clearer")]
#![allow(clippy::needless_pass_by_value, reason = "API design choice")]
#![allow(clippy::let_underscore_must_use, reason = "Intentionally ignoring results")]
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
#![allow(clippy::shadow_unrelated, reason = "Shadowing in match arms is idiomatic")]
#![allow(clippy::shadow_reuse, reason = "Shadowing for unwrapping is idiomatic")]
#![allow(clippy::exhaustive_enums, reason = "MCP tools are stable")]
#![allow(clippy::exhaustive_structs, reason = "MCP types are stable")]
#![allow(clippy::single_call_fn, reason = "Handler functions improve readability")]

mod handlers;
mod tools;

use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_infinite::InfiniteMemory;
use opencode_mem_storage::Storage;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;
use tokio::runtime::Handle;

pub use tools::McpTool;

use handlers::handle_tool_call;
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

#[derive(Serialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
}

pub fn run_mcp_server(
    storage: Arc<Storage>,
    embeddings: Option<Arc<EmbeddingService>>,
    infinite_mem: Option<Arc<InfiniteMemory>>,
    handle: Handle,
) {
    tracing::info!("MCP server starting on stdio");
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let reader = BufReader::new(stdin.lock());

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

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
                    error: Some(McpError { code: -32700, message: format!("Parse error: {e}") }),
                };
                if let Ok(json) = serde_json::to_string(&error_response) {
                    let _ = writeln!(stdout, "{json}");
                    let _ = stdout.flush();
                }
                continue;
            },
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
                    let _ = writeln!(stdout, "{json}");
                    let _ = stdout.flush();
                }
                continue;
            },
        };

        if let Some(response) = handle_request(
            &storage,
            embeddings.as_deref(),
            infinite_mem.as_deref(),
            &handle,
            &request,
        ) {
            if let Ok(response_json) = serde_json::to_string(&response) {
                writeln!(stdout, "{response_json}").ok();
                stdout.flush().ok();
            }
        }
    }
}

fn handle_request(
    storage: &Storage,
    embeddings: Option<&EmbeddingService>,
    infinite_mem: Option<&InfiniteMemory>,
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
            handle_tool_call(storage, embeddings, infinite_mem, handle, &req.params, id)
        },
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
mod tests {
    use super::*;
    use handlers::{mcp_err, mcp_ok, mcp_text};

    #[test]
    fn test_mcp_tool_parse_valid() {
        assert_eq!(McpTool::parse("search"), Some(McpTool::Search));
        assert_eq!(McpTool::parse("timeline"), Some(McpTool::Timeline));
        assert_eq!(McpTool::parse("get_observations"), Some(McpTool::GetObservations));
        assert_eq!(McpTool::parse("memory_get"), Some(McpTool::MemoryGet));
        assert_eq!(McpTool::parse("memory_recent"), Some(McpTool::MemoryRecent));
        assert_eq!(McpTool::parse("memory_hybrid_search"), Some(McpTool::MemoryHybridSearch));
        assert_eq!(McpTool::parse("memory_semantic_search"), Some(McpTool::MemorySemanticSearch));
        assert_eq!(McpTool::parse("__IMPORTANT"), Some(McpTool::Important));
        assert_eq!(McpTool::parse("knowledge_search"), Some(McpTool::KnowledgeSearch));
        assert_eq!(McpTool::parse("knowledge_save"), Some(McpTool::KnowledgeSave));
        assert_eq!(McpTool::parse("knowledge_get"), Some(McpTool::KnowledgeGet));
        assert_eq!(McpTool::parse("knowledge_list"), Some(McpTool::KnowledgeList));
        assert_eq!(McpTool::parse("infinite_expand"), Some(McpTool::InfiniteExpand));
        assert_eq!(McpTool::parse("infinite_time_range"), Some(McpTool::InfiniteTimeRange));
        assert_eq!(McpTool::parse("infinite_drill_hour"), Some(McpTool::InfiniteDrillHour));
        assert_eq!(McpTool::parse("infinite_drill_day"), Some(McpTool::InfiniteDrillDay));
    }

    #[test]
    fn test_mcp_tool_parse_invalid() {
        assert_eq!(McpTool::parse("unknown_tool"), None);
        assert_eq!(McpTool::parse(""), None);
        assert_eq!(McpTool::parse("SEARCH"), None);
        assert_eq!(McpTool::parse("search "), None);
    }

    #[test]
    #[expect(clippy::indexing_slicing, reason = "test code with known structure")]
    fn test_mcp_ok_serialization() {
        let data = vec!["item1", "item2"];
        let result = mcp_ok(&data);
        assert!(result.get("content").is_some());
        assert_eq!(result["content"][0]["type"].as_str(), Some("text"));
        assert!(result.get("isError").is_none());
    }

    #[test]
    #[expect(clippy::indexing_slicing, reason = "test code with known structure")]
    #[expect(clippy::unwrap_used, reason = "test code")]
    fn test_mcp_err_format() {
        let result = mcp_err("test error");
        assert_eq!(result["isError"].as_bool(), Some(true));
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Error: test error"));
    }

    #[test]
    #[expect(clippy::indexing_slicing, reason = "test code with known structure")]
    fn test_mcp_text_format() {
        let result = mcp_text("hello world");
        assert_eq!(result["content"][0]["text"].as_str(), Some("hello world"));
        assert!(result.get("isError").is_none());
    }
}
