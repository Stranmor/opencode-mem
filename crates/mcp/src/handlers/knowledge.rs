use opencode_mem_storage::Storage;
use serde_json::json;

use super::{mcp_err, mcp_ok, mcp_text};
use crate::McpResponse;

pub(super) fn handle_knowledge_search(
    storage: &Storage,
    args: &serde_json::Value,
) -> serde_json::Value {
    let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
    let limit = args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(10) as usize;
    match storage.search_knowledge(query, limit) {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_knowledge_get(
    storage: &Storage,
    args: &serde_json::Value,
) -> serde_json::Value {
    let id_str = args.get("id").and_then(|i| i.as_str()).unwrap_or("");
    match storage.get_knowledge(id_str) {
        Ok(Some(knowledge)) => {
            let _ = storage.update_knowledge_usage(id_str);
            mcp_ok(&knowledge)
        },
        Ok(None) => mcp_text(&format!("Knowledge not found: {id_str}")),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_knowledge_list(
    storage: &Storage,
    args: &serde_json::Value,
) -> serde_json::Value {
    let knowledge_type = args
        .get("knowledge_type")
        .and_then(|t| t.as_str())
        .and_then(|s| s.parse::<opencode_mem_core::KnowledgeType>().ok());
    let limit = args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(20) as usize;
    match storage.list_knowledge(knowledge_type, limit) {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_knowledge_delete(
    storage: &Storage,
    args: &serde_json::Value,
) -> serde_json::Value {
    let id_str = args.get("id").and_then(|i| i.as_str()).unwrap_or("");
    match storage.delete_knowledge(id_str) {
        Ok(deleted) => mcp_ok(&json!({ "success": true, "id": id_str, "deleted": deleted })),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_knowledge_save(
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

    let input = opencode_mem_core::KnowledgeInput::new(
        knowledge_type,
        title,
        description,
        instructions,
        triggers,
        source_project,
        source_observation,
    );

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
