mod handlers;
mod tools;

use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_storage::Storage;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;

pub use tools::McpTool;

use handlers::handle_tool_call;
use tools::get_tools_json;

#[derive(Deserialize)]
struct McpRequest {
    #[allow(dead_code)]
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

pub fn run_mcp_server(storage: Arc<Storage>, embeddings: Option<Arc<EmbeddingService>>) {
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
                    jsonrpc: "2.0".to_string(),
                    id: json!(null),
                    result: None,
                    error: Some(McpError {
                        code: -32700,
                        message: format!("Parse error: {}", e),
                    }),
                };
                if let Ok(json) = serde_json::to_string(&error_response) {
                    let _ = writeln!(stdout, "{}", json);
                    let _ = stdout.flush();
                }
                continue;
            }
        };

        let request: McpRequest = match serde_json::from_value(json_value.clone()) {
            Ok(r) => r,
            Err(e) => {
                let error_response = McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: json_value.get("id").cloned().unwrap_or(json!(null)),
                    result: None,
                    error: Some(McpError {
                        code: -32600,
                        message: format!("Invalid Request: {}", e),
                    }),
                };
                if let Ok(json) = serde_json::to_string(&error_response) {
                    let _ = writeln!(stdout, "{}", json);
                    let _ = stdout.flush();
                }
                continue;
            }
        };

        if let Some(response) = handle_request(&storage, embeddings.as_deref(), &request) {
            if let Ok(response_json) = serde_json::to_string(&response) {
                writeln!(stdout, "{}", response_json).ok();
                stdout.flush().ok();
            }
        }
    }
}

fn handle_request(
    storage: &Storage,
    embeddings: Option<&EmbeddingService>,
    req: &McpRequest,
) -> Option<McpResponse> {
    let id = match &req.id {
        Some(id) => id.clone(),
        None => return None,
    };

    Some(match req.method.as_str() {
        "initialize" => McpResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "opencode-memory", "version": "0.1.0" }
            })),
            error: None,
        },
        "tools/list" => McpResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(get_tools_json()),
            error: None,
        },
        "tools/call" => handle_tool_call(storage, embeddings, &req.params, id),
        _ => McpResponse {
            jsonrpc: "2.0".to_string(),
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
        assert_eq!(
            McpTool::parse("get_observations"),
            Some(McpTool::GetObservations)
        );
        assert_eq!(McpTool::parse("memory_get"), Some(McpTool::MemoryGet));
        assert_eq!(McpTool::parse("memory_recent"), Some(McpTool::MemoryRecent));
        assert_eq!(
            McpTool::parse("memory_hybrid_search"),
            Some(McpTool::MemoryHybridSearch)
        );
        assert_eq!(
            McpTool::parse("memory_semantic_search"),
            Some(McpTool::MemorySemanticSearch)
        );
        assert_eq!(McpTool::parse("__IMPORTANT"), Some(McpTool::Important));
        assert_eq!(
            McpTool::parse("knowledge_search"),
            Some(McpTool::KnowledgeSearch)
        );
        assert_eq!(
            McpTool::parse("knowledge_save"),
            Some(McpTool::KnowledgeSave)
        );
        assert_eq!(McpTool::parse("knowledge_get"), Some(McpTool::KnowledgeGet));
        assert_eq!(
            McpTool::parse("knowledge_list"),
            Some(McpTool::KnowledgeList)
        );
        assert_eq!(
            McpTool::parse("infinite_expand"),
            Some(McpTool::InfiniteExpand)
        );
        assert_eq!(
            McpTool::parse("infinite_time_range"),
            Some(McpTool::InfiniteTimeRange)
        );
        assert_eq!(
            McpTool::parse("infinite_drill_hour"),
            Some(McpTool::InfiniteDrillHour)
        );
        assert_eq!(
            McpTool::parse("infinite_drill_day"),
            Some(McpTool::InfiniteDrillDay)
        );
    }

    #[test]
    fn test_mcp_tool_parse_invalid() {
        assert_eq!(McpTool::parse("unknown_tool"), None);
        assert_eq!(McpTool::parse(""), None);
        assert_eq!(McpTool::parse("SEARCH"), None);
        assert_eq!(McpTool::parse("search "), None);
    }

    #[test]
    fn test_mcp_ok_serialization() {
        let data = vec!["item1", "item2"];
        let result = mcp_ok(&data);
        assert!(result.get("content").is_some());
        assert_eq!(result["content"][0]["type"].as_str(), Some("text"));
        assert!(result.get("isError").is_none());
    }

    #[test]
    fn test_mcp_err_format() {
        let result = mcp_err("test error");
        assert_eq!(result["isError"].as_bool(), Some(true));
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Error: test error"));
    }

    #[test]
    fn test_mcp_text_format() {
        let result = mcp_text("hello world");
        assert_eq!(result["content"][0]["text"].as_str(), Some("hello world"));
        assert!(result.get("isError").is_none());
    }
}
