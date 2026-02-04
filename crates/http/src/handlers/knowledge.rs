use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use opencode_mem_core::{GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType};

use crate::api_types::{KnowledgeQuery, KnowledgeUsageResponse, SaveKnowledgeRequest};
use crate::blocking::{blocking_json, blocking_result};
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
    blocking_json(move || storage.list_knowledge(knowledge_type, limit)).await
}

pub async fn search_knowledge(
    State(state): State<Arc<AppState>>,
    Query(query): Query<KnowledgeQuery>,
) -> Result<Json<Vec<KnowledgeSearchResult>>, StatusCode> {
    let storage = state.storage.clone();
    let q = query.q.clone();
    let limit = query.limit;
    blocking_json(move || storage.search_knowledge(&q, limit)).await
}

pub async fn get_knowledge_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<GlobalKnowledge>>, StatusCode> {
    let storage = state.storage.clone();
    blocking_json(move || storage.get_knowledge(&id)).await
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
    blocking_json(move || storage.save_knowledge(input)).await
}

pub async fn record_knowledge_usage(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<KnowledgeUsageResponse>, StatusCode> {
    let storage = state.storage.clone();
    let id_clone = id.clone();
    blocking_result(move || storage.update_knowledge_usage(&id_clone)).await?;
    Ok(Json(KnowledgeUsageResponse {
        success: true,
        id,
    }))
}
