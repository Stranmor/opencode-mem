use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use opencode_mem_core::{Observation, SearchResult, SessionSummary, ToolCall, UserPrompt};
use opencode_mem_storage::PaginatedResult;

use crate::api_types::{
    BatchRequest, ObserveBatchResponse, ObserveResponse, PaginationQuery, SearchQuery,
    TimelineQuery,
};
use crate::blocking::{blocking_json, blocking_result};
use crate::AppState;

pub async fn observe(
    State(state): State<Arc<AppState>>,
    Json(tool_call): Json<ToolCall>,
) -> Result<Json<ObserveResponse>, StatusCode> {
    // Serialize tool_call.input as tool_input for queue processing
    let tool_input = serde_json::to_string(&tool_call.input).ok();
    let storage = Arc::clone(&state.storage);
    let session_id = tool_call.session_id.clone();
    let tool_name = tool_call.tool.clone();
    let tool_response = tool_call.output.clone();
    let project = tool_call.project.clone();

    let message_id = blocking_result(move || {
        storage.queue_message(
            &session_id,
            Some(&tool_name),
            tool_input.as_deref(),
            Some(&tool_response),
            project.as_deref(),
        )
    })
    .await?;

    Ok(Json(ObserveResponse { id: message_id.to_string(), queued: true }))
}

pub async fn observe_batch(
    State(state): State<Arc<AppState>>,
    Json(tool_calls): Json<Vec<ToolCall>>,
) -> Result<Json<ObserveBatchResponse>, StatusCode> {
    let total = tool_calls.len();
    let storage = Arc::clone(&state.storage);
    let queued = blocking_result(move || {
        let mut count = 0usize;
        for tool_call in &tool_calls {
            let tool_input = serde_json::to_string(&tool_call.input).ok();
            match storage.queue_message(
                &tool_call.session_id,
                Some(&tool_call.tool),
                tool_input.as_deref(),
                Some(&tool_call.output),
                tool_call.project.as_deref(),
            ) {
                Ok(_id) => count = count.saturating_add(1),
                Err(e) => {
                    tracing::error!("Failed to queue tool call {}: {}", tool_call.tool, e);
                },
            }
        }
        Ok(count)
    })
    .await?;
    Ok(Json(ObserveBatchResponse { queued, total }))
}

pub async fn get_observation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<Observation>>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    blocking_json(move || storage.get_by_id(&id)).await
}

pub async fn get_recent(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    let limit = query.limit;
    blocking_json(move || storage.get_recent(limit)).await
}

pub async fn get_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    let from = query.from.clone();
    let to = query.to.clone();
    let limit = query.limit;
    blocking_json(move || storage.get_timeline(from.as_deref(), to.as_deref(), limit)).await
}

pub async fn get_observations_batch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchRequest>,
) -> Result<Json<Vec<Observation>>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    let ids = req.ids;
    blocking_json(move || storage.get_observations_by_ids(&ids)).await
}

pub async fn get_observations_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<Observation>>, StatusCode> {
    let limit = query.limit.min(100);
    let storage = Arc::clone(&state.storage);
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
    let storage = Arc::clone(&state.storage);
    let offset = query.offset;
    let project = query.project.clone();
    blocking_json(move || storage.get_summaries_paginated(offset, limit, project.as_deref())).await
}

pub async fn get_prompts_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<UserPrompt>>, StatusCode> {
    let limit = query.limit.min(100);
    let storage = Arc::clone(&state.storage);
    let offset = query.offset;
    let project = query.project.clone();
    blocking_json(move || storage.get_prompts_paginated(offset, limit, project.as_deref())).await
}

pub async fn get_session_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<SessionSummary>>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    blocking_json(move || storage.get_session_summary(&id)).await
}

pub async fn get_prompt_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<UserPrompt>>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    blocking_json(move || storage.get_prompt_by_id(&id)).await
}
