use opencode_mem_core::MAX_BATCH_IDS;
use opencode_mem_core::{NoiseLevel, ObservationType};
use opencode_mem_service::{ObservationService, PendingWrite, PendingWriteQueue, SearchService};
use serde_json::json;
use std::str::FromStr;

use super::{
    cb_fast_fail_read, cb_fast_fail_write, degrade_read_err, degrade_write_err, mcp_err, mcp_ok,
};

pub(super) async fn handle_search(
    search_service: &SearchService,
    args: &serde_json::Value,
    limit: usize,
) -> serde_json::Value {
    let cb = search_service.circuit_breaker();
    if let Some(degraded) = cb_fast_fail_read::<Vec<opencode_mem_core::SearchResult>>(cb) {
        return degraded;
    }
    let query = args
        .get("query")
        .and_then(|q| q.as_str())
        .filter(|s| !s.is_empty());

    let project = args.get("project").and_then(|p| p.as_str());
    let obs_type = args.get("type").and_then(|t| t.as_str());
    let from = args
        .get("from")
        .and_then(|f| f.as_str())
        .filter(|s| !s.is_empty());
    let to = args
        .get("to")
        .and_then(|t| t.as_str())
        .filter(|s| !s.is_empty());

    match search_service
        .smart_search(query, project, obs_type, from, to, limit)
        .await
    {
        Ok(results) => {
            cb.record_success();
            mcp_ok(&results)
        }
        Err(e) => degrade_read_err::<Vec<opencode_mem_core::SearchResult>>(e, cb),
    }
}

pub(super) async fn handle_timeline(
    search_service: &SearchService,
    args: &serde_json::Value,
    limit: usize,
) -> serde_json::Value {
    let cb = search_service.circuit_breaker();
    if let Some(degraded) = cb_fast_fail_read::<Vec<opencode_mem_core::SearchResult>>(cb) {
        return degraded;
    }
    let from = args
        .get("from")
        .and_then(|f| f.as_str())
        .filter(|s| !s.is_empty());
    let to = args
        .get("to")
        .and_then(|t| t.as_str())
        .filter(|s| !s.is_empty());

    match search_service.get_timeline(from, to, limit).await {
        Ok(results) => {
            cb.record_success();
            mcp_ok(&results)
        }
        Err(e) => degrade_read_err::<Vec<opencode_mem_core::SearchResult>>(e, cb),
    }
}

pub(super) async fn handle_get_observations(
    search_service: &SearchService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let ids: Vec<String> = args
        .get("ids")
        .and_then(|i| i.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return mcp_err("ids array is required and must not be empty");
    }
    if ids.len() > MAX_BATCH_IDS {
        return mcp_err(format!(
            "ids array exceeds maximum of {MAX_BATCH_IDS} items"
        ));
    }
    let cb = search_service.circuit_breaker();
    if let Some(degraded) = cb_fast_fail_read::<Vec<opencode_mem_core::Observation>>(cb) {
        return degraded;
    }
    match search_service.get_observations_by_ids(&ids).await {
        Ok(results) => {
            cb.record_success();
            mcp_ok(&results)
        }
        Err(e) => degrade_read_err::<Vec<opencode_mem_core::Observation>>(e, cb),
    }
}

pub(super) async fn handle_memory_get(
    search_service: &SearchService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let Some(id_str) = args
        .get("id")
        .and_then(|i| i.as_str())
        .filter(|s| !s.is_empty())
    else {
        return mcp_err("'id' parameter is required and must not be empty");
    };
    let cb = search_service.circuit_breaker();
    if let Some(degraded) = cb_fast_fail_read::<Vec<opencode_mem_core::Observation>>(cb) {
        return degraded;
    }
    match search_service.get_observation_by_id(id_str).await {
        Ok(Some(obs)) => {
            cb.record_success();
            mcp_ok(&obs)
        }
        Ok(None) => {
            cb.record_success();
            mcp_ok(&serde_json::Value::Null)
        }
        Err(e) if e.is_db_unavailable() || e.is_transient() => {
            cb.record_failure();
            tracing::warn!(error = %e, "MCP read: database unavailable, returning empty array");
            mcp_ok(&Vec::<opencode_mem_core::Observation>::new())
        }
        Err(e) => mcp_err(e),
    }
}

pub(super) async fn handle_memory_recent(
    search_service: &SearchService,
    _args: &serde_json::Value,
    limit: usize,
) -> serde_json::Value {
    let cb = search_service.circuit_breaker();
    if let Some(degraded) = cb_fast_fail_read::<Vec<opencode_mem_core::Observation>>(cb) {
        return degraded;
    }
    match search_service.get_recent_observations(limit).await {
        Ok(results) => {
            cb.record_success();
            mcp_ok(&results)
        }
        Err(e) => degrade_read_err::<Vec<opencode_mem_core::Observation>>(e, cb),
    }
}

pub(super) async fn handle_hybrid_search(
    search_service: &SearchService,
    args: &serde_json::Value,
    limit: usize,
) -> serde_json::Value {
    let Some(query) = args
        .get("query")
        .and_then(|q| q.as_str())
        .filter(|s| !s.is_empty())
    else {
        return mcp_err("'query' parameter is required and must not be empty");
    };
    let cb = search_service.circuit_breaker();
    if let Some(degraded) = cb_fast_fail_read::<Vec<opencode_mem_core::SearchResult>>(cb) {
        return degraded;
    }
    match search_service.hybrid_search(query, limit).await {
        Ok(results) => {
            cb.record_success();
            mcp_ok(&results)
        }
        Err(e) => degrade_read_err::<Vec<opencode_mem_core::SearchResult>>(e, cb),
    }
}

pub(super) async fn handle_semantic_search(
    search_service: &SearchService,
    args: &serde_json::Value,
    limit: usize,
) -> serde_json::Value {
    let Some(query) = args
        .get("query")
        .and_then(|q| q.as_str())
        .filter(|s| !s.is_empty())
    else {
        return mcp_err("'query' parameter is required and must not be empty");
    };
    let cb = search_service.circuit_breaker();
    if let Some(degraded) = cb_fast_fail_read::<Vec<opencode_mem_core::SearchResult>>(cb) {
        return degraded;
    }
    match search_service
        .semantic_search_with_fallback(query, limit)
        .await
    {
        Ok(results) => {
            cb.record_success();
            mcp_ok(&results)
        }
        Err(e) => degrade_read_err::<Vec<opencode_mem_core::SearchResult>>(e, cb),
    }
}

pub(super) async fn handle_save_memory(
    observation_service: &opencode_mem_service::ObservationService,
    pending_writes: &PendingWriteQueue,
    args: &serde_json::Value,
) -> serde_json::Value {
    let raw_text = match args.get("text").and_then(|t| t.as_str()) {
        Some(text) => text.trim(),
        None => return mcp_err("text is required and must be a string"),
    };
    if raw_text.is_empty() {
        return mcp_err("text is required and must not be empty");
    }

    let title = args.get("title").and_then(|t| t.as_str());
    let project = args.get("project").and_then(|p| p.as_str());
    let observation_type = match args
        .get("observation_type")
        .and_then(|t| t.as_str())
        .filter(|s| !s.is_empty())
    {
        Some(raw) => match ObservationType::from_str(raw) {
            Ok(value) => Some(value),
            Err(_) => {
                return mcp_err(format!(
                    "invalid observation_type: {raw} (allowed: {})",
                    ObservationType::ALL_VARIANTS_STR
                ));
            }
        },
        None => None,
    };
    let noise_level = match args
        .get("noise_level")
        .and_then(|n| n.as_str())
        .filter(|s| !s.is_empty())
    {
        Some(raw) => match NoiseLevel::from_str(raw) {
            Ok(value) => Some(value),
            Err(_) => {
                return mcp_err(format!(
                    "invalid noise_level: {raw} (allowed: {})",
                    NoiseLevel::ALL_VARIANTS_STR
                ));
            }
        },
        None => None,
    };

    let cb = observation_service.circuit_breaker();
    if let Some(degraded) = cb_fast_fail_write(cb) {
        pending_writes.push(PendingWrite::SaveMemory {
            text: raw_text.to_owned(),
            title: title.map(ToOwned::to_owned),
            project: project.map(ToOwned::to_owned),
            observation_type,
            noise_level,
        });
        return degraded;
    }

    match observation_service
        .save_memory(raw_text, title, project, observation_type, noise_level)
        .await
    {
        Ok(opencode_mem_service::SaveMemoryResult::Created(obs)) => {
            cb.record_success();
            mcp_ok(&obs)
        }
        Ok(opencode_mem_service::SaveMemoryResult::Duplicate(obs)) => {
            cb.record_success();
            mcp_ok(&obs)
        }
        Ok(opencode_mem_service::SaveMemoryResult::Filtered) => {
            cb.record_success();
            mcp_ok(&serde_json::json!({ "filtered": true, "reason": "low-value" }))
        }
        Err(e) if e.is_db_unavailable() || e.is_transient() => {
            let cb = observation_service.circuit_breaker();
            cb.record_failure();
            pending_writes.push(PendingWrite::SaveMemory {
                text: raw_text.to_owned(),
                title: title.map(ToOwned::to_owned),
                project: project.map(ToOwned::to_owned),
                observation_type,
                noise_level,
            });
            tracing::warn!(
                pending_count = pending_writes.len(),
                "MCP write: database unavailable, buffered save_memory for later flush"
            );
            mcp_ok(&serde_json::json!({ "success": false, "degraded": true, "buffered": true }))
        }
        Err(e) => mcp_err(e),
    }
}

pub(super) async fn handle_memory_delete(
    observation_service: &ObservationService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let Some(id_str) = args
        .get("id")
        .and_then(|i| i.as_str())
        .filter(|s| !s.is_empty())
    else {
        return mcp_err("'id' parameter is required and must not be empty");
    };
    let cb = observation_service.circuit_breaker();
    if let Some(degraded) = cb_fast_fail_write(cb) {
        return degraded;
    }
    match observation_service.delete_observation(id_str).await {
        Ok(deleted) => {
            cb.record_success();
            mcp_ok(&json!({ "success": deleted, "id": id_str, "deleted": deleted }))
        }
        Err(e) => degrade_write_err(e, cb),
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
#[expect(clippy::indexing_slicing, reason = "test code — asserts guard length")]
#[path = "memory_tests.rs"]
mod tests;
