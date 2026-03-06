use opencode_mem_core::{NoiseLevel, ObservationType};
use opencode_mem_service::{ObservationService, PendingWrite, PendingWriteQueue};
use serde_json::json;
use std::str::FromStr;

use crate::handlers::{cb_fast_fail_write, degrade_write_err, mcp_err, mcp_ok};

pub(in crate::handlers) async fn handle_save_memory(
    observation_service: &ObservationService,
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
    let observation_type = match args.get("observation_type") {
        Some(serde_json::Value::Null) | None => None,
        Some(serde_json::Value::String(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return mcp_err("observation_type cannot be empty if provided");
            }
            match ObservationType::from_str(trimmed) {
                Ok(value) => Some(value),
                Err(_) => {
                    return mcp_err(format!(
                        "invalid observation_type: {trimmed} (allowed: {})",
                        ObservationType::ALL_VARIANTS_STR
                    ));
                }
            }
        }
        Some(_) => return mcp_err("observation_type must be a string"),
    };
    let noise_level = match args.get("noise_level") {
        Some(serde_json::Value::Null) | None => None,
        Some(serde_json::Value::String(raw)) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return mcp_err("noise_level cannot be empty if provided");
            }
            match NoiseLevel::from_str(trimmed) {
                Ok(value) => Some(value),
                Err(_) => {
                    return mcp_err(format!(
                        "invalid noise_level: {trimmed} (allowed: {})",
                        NoiseLevel::ALL_VARIANTS_STR
                    ));
                }
            }
        }
        Some(_) => return mcp_err("noise_level must be a string"),
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
            mcp_ok(&json!({ "filtered": true, "reason": "low-value" }))
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
            mcp_ok(&json!({ "success": false, "degraded": true, "buffered": true }))
        }
        Err(e) => mcp_err(e),
    }
}

pub(in crate::handlers) async fn handle_memory_delete(
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
