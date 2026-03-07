use opencode_mem_service::KnowledgeService;
use serde_json::json;
use uuid::Uuid;

use super::{cb_fast_fail_read, cb_fast_fail_write, degrade_read_err, mcp_err, mcp_ok};

pub(super) async fn handle_knowledge_search(
    knowledge_service: &KnowledgeService,
    args: &serde_json::Value,
    limit: usize,
) -> serde_json::Value {
    let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
    let cb = knowledge_service.circuit_breaker();
    if let Some(degraded) = cb_fast_fail_read::<Vec<opencode_mem_core::KnowledgeSearchResult>>(cb) {
        return degraded;
    }
    match knowledge_service.search_knowledge(query, limit).await {
        Ok(results) => mcp_ok(&results),
        Err(e) => degrade_read_err::<Vec<opencode_mem_core::KnowledgeSearchResult>>(e, cb),
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
    let cb = knowledge_service.circuit_breaker();
    if let Some(degraded) = cb_fast_fail_read::<Vec<opencode_mem_core::GlobalKnowledge>>(cb) {
        return degraded;
    }
    match knowledge_service.get_knowledge(id_str).await {
        Ok(Some(knowledge)) => mcp_ok(&knowledge),
        Ok(None) => mcp_ok(&serde_json::Value::Null),
        Err(e) if e.is_db_unavailable() || e.is_transient() => {
            cb.record_failure();
            tracing::warn!(error = %e, "MCP read: database unavailable, returning empty array for knowledge");
            mcp_ok(&Vec::<opencode_mem_core::GlobalKnowledge>::new())
        }
        Err(e) => mcp_err(e),
    }
}

pub(super) async fn handle_knowledge_list(
    knowledge_service: &KnowledgeService,
    args: &serde_json::Value,
    limit: usize,
) -> serde_json::Value {
    let knowledge_type = match args.get("knowledge_type").and_then(|t| t.as_str()) {
        Some(s) => match s.parse::<opencode_mem_core::KnowledgeType>() {
            Ok(kt) => Some(kt),
            Err(e) => return mcp_err(format!("Invalid knowledge_type: {e}")),
        },
        None => None,
    };
    let cb = knowledge_service.circuit_breaker();
    if let Some(degraded) = cb_fast_fail_read::<Vec<opencode_mem_core::GlobalKnowledge>>(cb) {
        return degraded;
    }
    match knowledge_service
        .list_knowledge(knowledge_type, limit)
        .await
    {
        Ok(results) => {
            cb.record_success();
            mcp_ok(&results)
        }
        Err(e) => degrade_read_err::<Vec<opencode_mem_core::GlobalKnowledge>>(e, cb),
    }
}

pub(super) async fn handle_knowledge_delete(
    knowledge_service: &KnowledgeService,
    pending_writes: &opencode_mem_service::PendingWriteQueue,
    args: &serde_json::Value,
) -> serde_json::Value {
    let id_str = match args.get("id").and_then(|i| i.as_str()) {
        Some(id) => id,
        None => return mcp_err("id is required"),
    };
    let cb = knowledge_service.circuit_breaker();
    if let Some(degraded) = cb_fast_fail_write(cb) {
        pending_writes.push(opencode_mem_service::PendingWrite::DeleteKnowledge {
            id: id_str.to_owned(),
        });
        return degraded;
    }
    match knowledge_service.delete_knowledge(id_str).await {
        Ok(deleted) => {
            cb.record_success();
            mcp_ok(&json!({ "success": deleted, "id": id_str, "deleted": deleted }))
        }
        Err(e) if e.is_db_unavailable() || e.is_transient() => {
            cb.record_failure();
            pending_writes.push(opencode_mem_service::PendingWrite::DeleteKnowledge {
                id: id_str.to_owned(),
            });
            mcp_ok(&json!({ "success": true, "degraded": true, "buffered": true, "id": id_str }))
        }
        Err(e) => mcp_err(e),
    }
}

pub(super) async fn handle_knowledge_save(
    knowledge_service: &KnowledgeService,
    pending_writes: &opencode_mem_service::PendingWriteQueue,
    args: &serde_json::Value,
) -> serde_json::Value {
    let knowledge_type_str = args
        .get("knowledge_type")
        .and_then(|t| t.as_str())
        .unwrap_or("skill");
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
    let instructions = args
        .get("instructions")
        .and_then(|i| i.as_str())
        .map(ToOwned::to_owned);
    let triggers: Vec<String> = args
        .get("triggers")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let source_project = args
        .get("source_project")
        .and_then(|p| p.as_str())
        .map(ToOwned::to_owned);
    let source_observation = args
        .get("source_observation")
        .and_then(|o| o.as_str())
        .map(ToOwned::to_owned);

    let input = opencode_mem_core::KnowledgeInput::new(
        knowledge_type,
        opencode_mem_core::sanitize_input(&title),
        opencode_mem_core::sanitize_input(&description),
        instructions
            .as_deref()
            .map(opencode_mem_core::sanitize_input),
        triggers
            .iter()
            .map(|s| opencode_mem_core::sanitize_input(s))
            .collect(),
        source_project
            .as_deref()
            .map(opencode_mem_core::sanitize_input),
        source_observation
            .as_deref()
            .map(opencode_mem_core::sanitize_input),
    );

    let id = Uuid::new_v4().to_string();
    let cb = knowledge_service.circuit_breaker();
    if let Some(degraded) = cb_fast_fail_write(cb) {
        pending_writes.push(opencode_mem_service::PendingWrite::SaveKnowledge {
            id: id.clone(),
            input: input.clone(),
        });
        return degraded;
    }
    match knowledge_service
        .save_knowledge_with_id(&id, input.clone())
        .await
    {
        Ok(knowledge) => {
            cb.record_success();
            mcp_ok(&knowledge)
        }
        Err(e) if e.is_db_unavailable() || e.is_transient() => {
            cb.record_failure();
            pending_writes.push(opencode_mem_service::PendingWrite::SaveKnowledge {
                id: id.clone(),
                input,
            });
            mcp_ok(&json!({ "success": true, "degraded": true, "buffered": true, "id": id }))
        }
        Err(e) => mcp_err(e),
    }
}
