use opencode_mem_storage::Storage;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::sync::Arc;

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
struct McpResponse {
    jsonrpc: String,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McpError>,
}

#[derive(Serialize)]
struct McpError {
    code: i32,
    message: String,
}

pub fn run_mcp_server(storage: Arc<Storage>) {
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

        let request: McpRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let response = handle_request(&storage, &request);
        let response_json = serde_json::to_string(&response).unwrap();
        writeln!(stdout, "{}", response_json).ok();
        stdout.flush().ok();
    }
}

fn handle_request(storage: &Storage, req: &McpRequest) -> McpResponse {
    let id = req.id.clone().unwrap_or(json!(null));

    match req.method.as_str() {
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
            result: Some(json!({
                "tools": [
                    {
                        "name": "memory_search",
                        "description": "Search through past session memories",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": { "type": "string", "description": "Search query" },
                                "limit": { "type": "integer", "default": 10 }
                            },
                            "required": ["query"]
                        }
                    },
                    {
                        "name": "memory_get",
                        "description": "Get full observation details by ID",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string", "description": "Observation ID" }
                            },
                            "required": ["id"]
                        }
                    },
                    {
                        "name": "memory_recent",
                        "description": "Get recent observations",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "limit": { "type": "integer", "default": 10 }
                            }
                        }
                    },
                    {
                        "name": "memory_hybrid_search",
                        "description": "Hybrid search combining FTS5 and keyword matching",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": { "type": "string", "description": "Search query (supports multiple words)" },
                                "limit": { "type": "integer", "default": 10 }
                            },
                            "required": ["query"]
                        }
                    },
                    {
                        "name": "memory_timeline",
                        "description": "Get observations within a time range",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "from": { "type": "string", "description": "Start date (ISO 8601)" },
                                "to": { "type": "string", "description": "End date (ISO 8601)" },
                                "limit": { "type": "integer", "default": 50 }
                            }
                        }
                    }
                ]
            })),
            error: None,
        },
        "tools/call" => handle_tool_call(storage, &req.params, id),
        _ => McpResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({})),
            error: None,
        },
    }
}

fn handle_tool_call(
    storage: &Storage,
    params: &serde_json::Value,
    id: serde_json::Value,
) -> McpResponse {
    let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = match tool_name {
        "memory_search" => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(10) as usize;
            match storage.search(query, limit) {
                Ok(results) => {
                    json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&results).unwrap() }] })
                }
                Err(e) => {
                    json!({ "content": [{ "type": "text", "text": format!("Error: {}", e) }], "isError": true })
                }
            }
        }
        "memory_get" => {
            let obs_id = args.get("id").and_then(|i| i.as_str()).unwrap_or("");
            match storage.get_by_id(obs_id) {
                Ok(Some(obs)) => {
                    json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&obs).unwrap() }] })
                }
                Ok(None) => {
                    json!({ "content": [{ "type": "text", "text": "Observation not found" }] })
                }
                Err(e) => {
                    json!({ "content": [{ "type": "text", "text": format!("Error: {}", e) }], "isError": true })
                }
            }
        }
        "memory_recent" => {
            let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(10) as usize;
            match storage.get_recent(limit) {
                Ok(results) => {
                    json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&results).unwrap() }] })
                }
                Err(e) => {
                    json!({ "content": [{ "type": "text", "text": format!("Error: {}", e) }], "isError": true })
                }
            }
        }
        "memory_hybrid_search" => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(10) as usize;
            match storage.hybrid_search(query, limit) {
                Ok(results) => {
                    json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&results).unwrap() }] })
                }
                Err(e) => {
                    json!({ "content": [{ "type": "text", "text": format!("Error: {}", e) }], "isError": true })
                }
            }
        }
        "memory_timeline" => {
            let from = args.get("from").and_then(|f| f.as_str());
            let to = args.get("to").and_then(|t| t.as_str());
            let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(50) as usize;
            match storage.get_timeline(from, to, limit) {
                Ok(results) => {
                    json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&results).unwrap() }] })
                }
                Err(e) => {
                    json!({ "content": [{ "type": "text", "text": format!("Error: {}", e) }], "isError": true })
                }
            }
        }
        _ => json!({ "content": [{ "type": "text", "text": "Unknown tool" }], "isError": true }),
    };

    McpResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
        error: None,
    }
}
