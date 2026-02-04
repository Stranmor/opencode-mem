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
    state.storage.get_by_id(&id).map(Json).map_err(|e| {
        tracing::error!("Get observation failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn get_recent(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    state
        .storage
        .get_recent(query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get recent failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    state
        .storage
        .get_timeline(query.from.as_deref(), query.to.as_deref(), query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get timeline failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_observations_batch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchRequest>,
) -> Result<Json<Vec<Observation>>, StatusCode> {
    state
        .storage
        .get_observations_by_ids(&req.ids)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Batch get observations failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_observations_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<Observation>>, StatusCode> {
    let limit = query.limit.min(100);
    state
        .storage
        .get_observations_paginated(query.offset, limit, query.project.as_deref())
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get observations paginated failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_summaries_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<SessionSummary>>, StatusCode> {
    let limit = query.limit.min(100);
    state
        .storage
        .get_summaries_paginated(query.offset, limit, query.project.as_deref())
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get summaries paginated failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_prompts_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<UserPrompt>>, StatusCode> {
    let limit = query.limit.min(100);
    state
        .storage
        .get_prompts_paginated(query.offset, limit, query.project.as_deref())
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get prompts paginated failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_session_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<SessionSummary>>, StatusCode> {
    state
        .storage
        .get_session_summary(&id)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get session by id failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_prompt_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<UserPrompt>>, StatusCode> {
    state.storage.get_prompt_by_id(&id).map(Json).map_err(|e| {
        tracing::error!("Get prompt by id failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}
