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
    state
        .storage
        .list_knowledge(knowledge_type, query.limit)
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
    state
        .storage
        .search_knowledge(&query.q, query.limit)
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
    state.storage.get_knowledge(&id).map(Json).map_err(|e| {
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

    state.storage.save_knowledge(input).map(Json).map_err(|e| {
        tracing::error!("Save knowledge failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn record_knowledge_usage(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<KnowledgeUsageResponse>, StatusCode> {
    state
        .storage
        .update_knowledge_usage(&id)
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
