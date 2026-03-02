use super::is_localhost;
use crate::api_error::ApiError;
use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;

use opencode_mem_core::{
    GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType, MAX_QUERY_LIMIT,
};

use crate::api_types::{KnowledgeQuery, KnowledgeUsageResponse, SaveKnowledgeRequest};
use crate::AppState;

pub async fn list_knowledge(
    State(state): State<Arc<AppState>>,
    Query(query): Query<KnowledgeQuery>,
) -> Result<Json<Vec<GlobalKnowledge>>, ApiError> {
    let knowledge_type = match query.knowledge_type.as_ref() {
        Some(s) => Some(
            s.parse::<KnowledgeType>().map_err(|_| ApiError::BadRequest("Bad Request".into()))?,
        ),
        None => None,
    };
    state
        .knowledge_service
        .list_knowledge(knowledge_type, query.limit.min(MAX_QUERY_LIMIT))
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("List knowledge error: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        })
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
        .search_knowledge(&query.q, query.limit.min(MAX_QUERY_LIMIT))
        .await
        .map_err(|e| {
            tracing::error!("Search knowledge error: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        })?;
    // Fire-and-forget: update usage_count for all returned results.
    let knowledge_service = state.knowledge_service.clone();
    let result_ids: Vec<String> = results.iter().map(|r| r.knowledge.id.clone()).collect();
    tokio::spawn(async move {
        for id in result_ids {
            let _ = knowledge_service.update_knowledge_usage(&id).await;
        }
    });
    Ok(Json(results))
}

pub async fn get_knowledge_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<GlobalKnowledge>, ApiError> {
    let knowledge = state.knowledge_service.get_knowledge(&id).await.map_err(|e| {
        tracing::error!("Get knowledge error: {}", e);
        ApiError::Internal(anyhow::anyhow!("Internal Error"))
    })?;
    knowledge.map(Json).ok_or(ApiError::NotFound("Not Found".into()))
}

pub async fn delete_knowledge(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !is_localhost(&addr) {
        return Err(ApiError::Forbidden("Forbidden".into()));
    }
    let deleted = state.knowledge_service.delete_knowledge(&id).await.map_err(|e| {
        tracing::error!("Delete knowledge error: {}", e);
        ApiError::Internal(anyhow::anyhow!("Internal Error"))
    })?;
    Ok(Json(json!({ "success": deleted, "id": id, "deleted": deleted })))
}

pub async fn save_knowledge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveKnowledgeRequest>,
) -> Result<Json<GlobalKnowledge>, ApiError> {
    let knowledge_type = req
        .knowledge_type
        .parse::<KnowledgeType>()
        .map_err(|_parse_err| ApiError::BadRequest("Bad Request".into()))?;

    let input = KnowledgeInput::new(
        knowledge_type,
        req.title,
        req.description,
        req.instructions,
        req.triggers,
        req.source_project,
        req.source_observation,
    );

    state.knowledge_service.save_knowledge(input).await.map(Json).map_err(|e| {
        tracing::error!("Save knowledge error: {}", e);
        ApiError::Internal(anyhow::anyhow!("Internal Error"))
    })
}

pub async fn record_knowledge_usage(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<KnowledgeUsageResponse>, ApiError> {
    state.knowledge_service.update_knowledge_usage(&id).await.map_err(|e| {
        tracing::error!("Update knowledge usage error: {}", e);
        ApiError::Internal(anyhow::anyhow!("Internal Error"))
    })?;
    Ok(Json(KnowledgeUsageResponse { success: true, id }))
}
