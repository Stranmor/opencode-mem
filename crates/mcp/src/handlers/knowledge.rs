use opencode_mem_service::KnowledgeService;
use serde_json::json;

use super::{mcp_err, mcp_ok, mcp_text, parse_limit};

pub(super) async fn handle_knowledge_search(
    knowledge_service: &KnowledgeService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
    let limit = parse_limit(args);
    match knowledge_service.search_knowledge(query, limit).await {
        Ok(results) => {
            for result in &results {
                let _ = knowledge_service.update_knowledge_usage(&result.knowledge.id).await;
            }
            mcp_ok(&results)
        },
        Err(e) => mcp_err(e),
    }
}

pub(super) async fn handle_knowledge_get(
    knowledge_service: &KnowledgeService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let id_str = match args.get("id").and_then(|i| i.as_str()) {
        Some(id) => id,
        None => return mcp_err("id is required"),
    };
    match knowledge_service.get_knowledge(id_str).await {
        Ok(Some(knowledge)) => {
            let _ = knowledge_service.update_knowledge_usage(id_str).await;
            mcp_ok(&knowledge)
        },
        Ok(None) => mcp_text(&format!("Knowledge not found: {id_str}")),
        Err(e) => mcp_err(e),
    }
}

pub(super) async fn handle_knowledge_list(
    knowledge_service: &KnowledgeService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let knowledge_type = match args.get("knowledge_type").and_then(|t| t.as_str()) {
        Some(s) => match s.parse::<opencode_mem_core::KnowledgeType>() {
            Ok(kt) => Some(kt),
            Err(e) => return mcp_err(format!("Invalid knowledge_type: {e}")),
        },
        None => None,
    };
    let limit = parse_limit(args);
    match knowledge_service.list_knowledge(knowledge_type, limit).await {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) async fn handle_knowledge_delete(
    knowledge_service: &KnowledgeService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let id_str = match args.get("id").and_then(|i| i.as_str()) {
        Some(id) => id,
        None => return mcp_err("id is required"),
    };
    match knowledge_service.delete_knowledge(id_str).await {
        Ok(deleted) => mcp_ok(&json!({ "success": deleted, "id": id_str, "deleted": deleted })),
        Err(e) => mcp_err(e),
    }
}

pub(super) async fn handle_knowledge_save(
    knowledge_service: &KnowledgeService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let knowledge_type_str = args.get("knowledge_type").and_then(|t| t.as_str()).unwrap_or("skill");
    let knowledge_type = match knowledge_type_str.parse::<opencode_mem_core::KnowledgeType>() {
        Ok(kt) => kt,
        Err(e) => return mcp_err(format!("Invalid knowledge_type: {e}")),
    };
    let title = match args.get("title").and_then(|t| t.as_str()) {
        Some(t) if !t.is_empty() => t.to_owned(),
        _ => return mcp_err("title is required and cannot be empty"),
    };
    let description = match args.get("description").and_then(|d| d.as_str()) {
        Some(d) if !d.is_empty() => d.to_owned(),
        _ => return mcp_err("description is required and cannot be empty"),
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

    match knowledge_service.save_knowledge(input).await {
        Ok(knowledge) => mcp_ok(&knowledge),
        Err(e) => mcp_err(e),
    }
}
