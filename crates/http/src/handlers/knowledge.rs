use crate::api_error::{ApiError, DegradedExt, OrDegraded};
use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State},
};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;

use opencode_mem_core::{GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult};

use crate::AppState;
use crate::api_types::{KnowledgeQuery, KnowledgeUsageResponse, SaveKnowledgeRequest};

pub async fn list_knowledge(
    State(state): State<Arc<AppState>>,
    Query(query): Query<KnowledgeQuery>,
) -> Result<Json<Vec<GlobalKnowledge>>, ApiError> {
    state
        .knowledge_service
        .list_knowledge(query.knowledge_type, query.limit)
        .await
        .or_degraded(Vec::<GlobalKnowledge>::new())
        .map(Json)
}

pub async fn search_knowledge(
    State(state): State<Arc<AppState>>,
    Query(query): Query<KnowledgeQuery>,
) -> Result<Json<Vec<KnowledgeSearchResult>>, ApiError> {
    if query.q.trim().is_empty() {
        return Err(ApiError::BadRequest("Bad Request".into()));
    }
    let results = state
        .knowledge_service
        .search_knowledge(&query.q, query.limit)
        .await
        .or_degraded(Vec::<KnowledgeSearchResult>::new())?;

    // Fire-and-forget: update usage_count for all returned results in one batch.
    let knowledge_service = state.knowledge_service.clone();
    let result_ids: Vec<String> = results.iter().map(|r| r.knowledge.id.clone()).collect();
    tokio::spawn(async move {
        let _ = knowledge_service
            .update_knowledge_usage_batch(&result_ids)
            .await;
    });
    Ok(Json(results))
}

pub async fn get_knowledge_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<GlobalKnowledge>, ApiError> {
    let knowledge = state
        .knowledge_service
        .get_knowledge(&id)
        .await
        .or_degraded(None::<GlobalKnowledge>)?;

    match knowledge {
        Some(k) => {
            let knowledge_service = state.knowledge_service.clone();
            let entry_id = id.clone();
            tokio::spawn(async move {
                let _ = knowledge_service.update_knowledge_usage(&entry_id).await;
            });
            Ok(Json(k))
        }
        None => Err(ApiError::NotFound("Not Found".into())),
    }
}

pub async fn delete_knowledge(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !super::check_admin_access(&addr, &headers, &state.config) {
        return Err(ApiError::Forbidden("Forbidden".into()));
    }
    let resp_body = state
        .knowledge_service
        .delete_knowledge(&id)
        .await
        .map(|deleted| json!({ "success": deleted, "id": id, "deleted": deleted }))
        .map_err(|e| {
            if e.is_db_unavailable() || e.is_transient() {
                state.queue_service.push_pending_write(
                    opencode_mem_service::PendingWrite::DeleteKnowledge { id: id.clone() },
                );
            }
            tracing::error!("Delete knowledge error: {}", e);
            ApiError::from(e)
        })
        .with_degraded_body(json!({ "success": true, "id": id, "deleted": true }))?;
    Ok(Json(resp_body))
}

pub async fn save_knowledge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveKnowledgeRequest>,
) -> Result<Json<GlobalKnowledge>, ApiError> {
    let title = req.title.clone();
    let description = req.description.clone();
    let knowledge_type = req.knowledge_type;

    let input = KnowledgeInput::new(
        req.knowledge_type,
        opencode_mem_core::sanitize_input(&req.title),
        opencode_mem_core::sanitize_input(&req.description),
        req.instructions
            .as_deref()
            .map(opencode_mem_core::sanitize_input),
        req.triggers
            .iter()
            .map(|s| opencode_mem_core::sanitize_input(s))
            .collect(),
        req.source_project
            .as_deref()
            .map(opencode_mem_core::sanitize_input),
        req.source_observation
            .as_deref()
            .map(opencode_mem_core::sanitize_input),
    );

    let id = uuid::Uuid::new_v4().to_string();

    state
        .knowledge_service
        .save_knowledge_with_id(&id, input.clone())
        .await
        .map(Json)
        .map_err(|e| {
            if e.is_db_unavailable() || e.is_transient() {
                state.queue_service.push_pending_write(
                    opencode_mem_service::PendingWrite::SaveKnowledge {
                        id: id.clone(),
                        input,
                    },
                );
            }
            tracing::error!("Save knowledge error: {}", e);
            ApiError::from(e)
        })
        .with_degraded_body(json!({
            "id": id,
            "knowledge_type": knowledge_type,
            "title": title,
            "description": description,
            "confidence": 0.5,
            "usage_count": 0,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "updated_at": chrono::Utc::now().to_rfc3339()
        }))
}

pub async fn record_knowledge_usage(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<KnowledgeUsageResponse>, ApiError> {
    state
        .knowledge_service
        .update_knowledge_usage(&id)
        .await
        .map_err(|e| {
            tracing::error!("Update knowledge usage error: {}", e);
            ApiError::from(e)
        })?;
    Ok(Json(KnowledgeUsageResponse { success: true, id }))
}

pub async fn run_confidence_lifecycle(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !super::check_admin_access(&addr, &headers, &state.config) {
        return Err(ApiError::Forbidden("Forbidden".into()));
    }
    let (decayed, archived) = state
        .knowledge_service
        .run_confidence_lifecycle()
        .await
        .map_err(|e| {
            tracing::error!("Knowledge confidence lifecycle error: {}", e);
            ApiError::from(e)
        })?;
    Ok(Json(json!({ "decayed": decayed, "archived": archived })))
}
