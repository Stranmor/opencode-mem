use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_storage::Storage;

use super::{mcp_err, mcp_ok, mcp_text};

pub(super) fn handle_search(storage: &Storage, args: &serde_json::Value) -> serde_json::Value {
    let query = args.get("query").and_then(|q| q.as_str());
    let limit = args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(50) as usize;
    let project = args.get("project").and_then(|p| p.as_str());
    let obs_type = args.get("type").and_then(|t| t.as_str());
    match storage.search_with_filters(query, project, obs_type, limit) {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_timeline(storage: &Storage, args: &serde_json::Value) -> serde_json::Value {
    let from = args.get("from").and_then(|f| f.as_str());
    let to = args.get("to").and_then(|t| t.as_str());
    let limit = args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(50) as usize;
    match storage.get_timeline(from, to, limit) {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_get_observations(
    storage: &Storage,
    args: &serde_json::Value,
) -> serde_json::Value {
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
}

pub(super) fn handle_memory_get(storage: &Storage, args: &serde_json::Value) -> serde_json::Value {
    let id_str = args.get("id").and_then(|i| i.as_str()).unwrap_or("");
    match storage.get_by_id(id_str) {
        Ok(Some(obs)) => mcp_ok(&obs),
        Ok(None) => mcp_text(&format!("Observation not found: {id_str}")),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_memory_recent(
    storage: &Storage,
    args: &serde_json::Value,
) -> serde_json::Value {
    let limit = args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(10) as usize;
    match storage.get_recent(limit) {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_hybrid_search(
    storage: &Storage,
    args: &serde_json::Value,
) -> serde_json::Value {
    let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
    let limit = args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(20) as usize;
    match storage.hybrid_search(query, limit) {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_semantic_search(
    storage: &Storage,
    embeddings: Option<&EmbeddingService>,
    args: &serde_json::Value,
) -> serde_json::Value {
    let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
    let limit = args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(20) as usize;

    match opencode_mem_search::run_semantic_search_with_fallback(storage, embeddings, query, limit)
    {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}
