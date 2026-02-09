use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde_json::json;
use std::sync::Arc;

use opencode_mem_core::{GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType};

use crate::api_types::{KnowledgeQuery, KnowledgeUsageResponse, SaveKnowledgeRequest};
use crate::blocking::{blocking_json, blocking_result};
use crate::AppState;

pub async fn list_knowledge(
    State(state): State<Arc<AppState>>,
    Query(query): Query<KnowledgeQuery>,
) -> Result<Json<Vec<GlobalKnowledge>>, StatusCode> {
    let knowledge_type = match query.knowledge_type.as_ref() {
        Some(s) => Some(s.parse::<KnowledgeType>().map_err(|_| StatusCode::BAD_REQUEST)?),
        None => None,
    };
    let storage = Arc::clone(&state.storage);
    let limit = query.limit;
    blocking_json(move || storage.list_knowledge(knowledge_type, limit)).await
}

pub async fn search_knowledge(
    State(state): State<Arc<AppState>>,
    Query(query): Query<KnowledgeQuery>,
) -> Result<Json<Vec<KnowledgeSearchResult>>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    let q = query.q.clone();
    let limit = query.limit;
    blocking_json(move || storage.search_knowledge(&q, limit)).await
}

pub async fn get_knowledge_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<GlobalKnowledge>>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    blocking_json(move || storage.get_knowledge(&id)).await
}

pub async fn delete_knowledge(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    blocking_json(move || {
        let deleted = storage.delete_knowledge(&id)?;
        Ok(json!({ "success": deleted, "id": id, "deleted": deleted }))
    })
    .await
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

    let storage = Arc::clone(&state.storage);
    blocking_json(move || storage.save_knowledge(input)).await
}

pub async fn record_knowledge_usage(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<KnowledgeUsageResponse>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    let id_clone = id.clone();
    blocking_result(move || storage.update_knowledge_usage(&id_clone)).await?;
    Ok(Json(KnowledgeUsageResponse { success: true, id }))
}
