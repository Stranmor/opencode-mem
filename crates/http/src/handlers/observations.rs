use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use opencode_mem_core::{Observation, SearchResult, SessionSummary, ToolCall, UserPrompt};
use opencode_mem_storage::PaginatedResult;

use crate::api_types::{
    BatchRequest, ObserveResponse, PaginationQuery, SearchQuery, TimelineQuery,
};
use crate::blocking::blocking_json;
use crate::AppState;

pub async fn observe(
    State(state): State<Arc<AppState>>,
    Json(tool_call): Json<ToolCall>,
) -> Result<Json<ObserveResponse>, StatusCode> {
    let id = uuid::Uuid::new_v4().to_string();

    let service = state.observation_service.clone();
    let semaphore = state.semaphore.clone();
    let id_clone = id.clone();
    tokio::spawn(async move {
        let permit = match semaphore.acquire().await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Semaphore closed, cannot process observation: {}", e);
                return;
            }
        };
        if let Err(e) = service.process(&id_clone, tool_call).await {
            tracing::error!("Failed to process observation: {}", e);
        }
        drop(permit);
    });

    Ok(Json(ObserveResponse { id, queued: true }))
}

pub async fn get_observation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<Observation>>, StatusCode> {
    let storage = state.storage.clone();
    blocking_json(move || storage.get_by_id(&id)).await
}

pub async fn get_recent(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let storage = state.storage.clone();
    let limit = query.limit;
    blocking_json(move || storage.get_recent(limit)).await
}

pub async fn get_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let storage = state.storage.clone();
    let from = query.from.clone();
    let to = query.to.clone();
    let limit = query.limit;
    blocking_json(move || storage.get_timeline(from.as_deref(), to.as_deref(), limit)).await
}

pub async fn get_observations_batch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchRequest>,
) -> Result<Json<Vec<Observation>>, StatusCode> {
    let storage = state.storage.clone();
    let ids = req.ids;
    blocking_json(move || storage.get_observations_by_ids(&ids)).await
}

pub async fn get_observations_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<Observation>>, StatusCode> {
    let limit = query.limit.min(100);
    let storage = state.storage.clone();
    let offset = query.offset;
    let project = query.project.clone();
    blocking_json(move || storage.get_observations_paginated(offset, limit, project.as_deref()))
        .await
}

pub async fn get_summaries_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<SessionSummary>>, StatusCode> {
    let limit = query.limit.min(100);
    let storage = state.storage.clone();
    let offset = query.offset;
    let project = query.project.clone();
    blocking_json(move || storage.get_summaries_paginated(offset, limit, project.as_deref())).await
}

pub async fn get_prompts_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<UserPrompt>>, StatusCode> {
    let limit = query.limit.min(100);
    let storage = state.storage.clone();
    let offset = query.offset;
    let project = query.project.clone();
    blocking_json(move || storage.get_prompts_paginated(offset, limit, project.as_deref())).await
}

pub async fn get_session_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<SessionSummary>>, StatusCode> {
    let storage = state.storage.clone();
    blocking_json(move || storage.get_session_summary(&id)).await
}

pub async fn get_prompt_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<UserPrompt>>, StatusCode> {
    let storage = state.storage.clone();
    blocking_json(move || storage.get_prompt_by_id(&id)).await
}
