use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use opencode_mem_core::{GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType};

use crate::api_types::{KnowledgeQuery, KnowledgeUsageResponse, SaveKnowledgeRequest};
use crate::AppState;

pub async fn list_knowledge(
    State(state): State<Arc<AppState>>,
    Query(query): Query<KnowledgeQuery>,
) -> Result<Json<Vec<GlobalKnowledge>>, StatusCode> {
    let knowledge_type = query
        .knowledge_type
        .as_ref()
        .and_then(|s| s.parse::<KnowledgeType>().ok());
    let storage = state.storage.clone();
    let limit = query.limit;
    tokio::task::spawn_blocking(move || storage.list_knowledge(knowledge_type, limit))
        .await
        .map_err(|e| {
            tracing::error!("List knowledge join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("List knowledge failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn search_knowledge(
    State(state): State<Arc<AppState>>,
    Query(query): Query<KnowledgeQuery>,
) -> Result<Json<Vec<KnowledgeSearchResult>>, StatusCode> {
    let storage = state.storage.clone();
    let q = query.q.clone();
    let limit = query.limit;
    tokio::task::spawn_blocking(move || storage.search_knowledge(&q, limit))
        .await
        .map_err(|e| {
            tracing::error!("Search knowledge join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search knowledge failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_knowledge_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<GlobalKnowledge>>, StatusCode> {
    let storage = state.storage.clone();
    tokio::task::spawn_blocking(move || storage.get_knowledge(&id))
        .await
        .map_err(|e| {
            tracing::error!("Get knowledge join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get knowledge failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn save_knowledge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveKnowledgeRequest>,
) -> Result<Json<GlobalKnowledge>, StatusCode> {
    let knowledge_type = req
        .knowledge_type
        .parse::<KnowledgeType>()
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let input = KnowledgeInput {
        knowledge_type,
        title: req.title,
        description: req.description,
        instructions: req.instructions,
        triggers: req.triggers,
        source_project: req.source_project,
        source_observation: req.source_observation,
    };

    let storage = state.storage.clone();
    tokio::task::spawn_blocking(move || storage.save_knowledge(input))
        .await
        .map_err(|e| {
            tracing::error!("Save knowledge join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("Save knowledge failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn record_knowledge_usage(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<KnowledgeUsageResponse>, StatusCode> {
    let storage = state.storage.clone();
    let id_clone = id.clone();
    tokio::task::spawn_blocking(move || storage.update_knowledge_usage(&id_clone))
        .await
        .map_err(|e| {
            tracing::error!("Record knowledge usage join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(|_| {
            Json(KnowledgeUsageResponse {
                success: true,
                id: id.clone(),
            })
        })
        .map_err(|e| {
            tracing::error!("Record knowledge usage failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}
