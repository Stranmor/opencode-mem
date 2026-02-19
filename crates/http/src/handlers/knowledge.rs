use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde_json::json;
use std::sync::Arc;

use opencode_mem_core::{
    GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType, MAX_QUERY_LIMIT,
};

use crate::AppState;
use crate::api_types::{KnowledgeQuery, KnowledgeUsageResponse, SaveKnowledgeRequest};

pub async fn list_knowledge(
    State(state): State<Arc<AppState>>,
    Query(query): Query<KnowledgeQuery>,
) -> Result<Json<Vec<GlobalKnowledge>>, StatusCode> {
    let knowledge_type = match query.knowledge_type.as_ref() {
        Some(s) => Some(s.parse::<KnowledgeType>().map_err(|_| StatusCode::BAD_REQUEST)?),
        None => None,
    };
    state
        .knowledge_service
        .list_knowledge(knowledge_type, query.limit.min(MAX_QUERY_LIMIT))
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("List knowledge error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn search_knowledge(
    State(state): State<Arc<AppState>>,
    Query(query): Query<KnowledgeQuery>,
) -> Result<Json<Vec<KnowledgeSearchResult>>, StatusCode> {
    if query.q.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let results = state
        .knowledge_service
        .search_knowledge(&query.q, query.limit.min(MAX_QUERY_LIMIT))
        .await
        .map_err(|e| {
            tracing::error!("Search knowledge error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    for result in &results {
        let _ = state.knowledge_service.update_knowledge_usage(&result.knowledge.id).await;
    }
    Ok(Json(results))
}

pub async fn get_knowledge_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<GlobalKnowledge>>, StatusCode> {
    state.knowledge_service.get_knowledge(&id).await.map(Json).map_err(|e| {
        tracing::error!("Get knowledge error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn delete_knowledge(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let deleted = state.knowledge_service.delete_knowledge(&id).await.map_err(|e| {
        tracing::error!("Delete knowledge error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(json!({ "success": deleted, "id": id, "deleted": deleted })))
}

pub async fn save_knowledge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveKnowledgeRequest>,
) -> Result<Json<GlobalKnowledge>, StatusCode> {
    let knowledge_type = req
        .knowledge_type
        .parse::<KnowledgeType>()
        .map_err(|_parse_err| StatusCode::BAD_REQUEST)?;

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
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn record_knowledge_usage(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<KnowledgeUsageResponse>, StatusCode> {
    state.knowledge_service.update_knowledge_usage(&id).await.map_err(|e| {
        tracing::error!("Update knowledge usage error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(KnowledgeUsageResponse { success: true, id }))
}
