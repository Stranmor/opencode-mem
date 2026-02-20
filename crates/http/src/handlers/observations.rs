use crate::api_error::ApiError;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use std::sync::Arc;

use opencode_mem_core::{
    Observation, ProjectFilter, SearchResult, SessionSummary, ToolCall, UserPrompt,
    filter_injected_memory, filter_private_content,
};
use opencode_mem_service::PaginatedResult;

use crate::AppState;
use crate::api_types::{
    BatchRequest, ObserveBatchResponse, ObserveResponse, PaginationQuery, SaveMemoryRequest,
    SearchQuery, TimelineQuery,
};

pub async fn observe(
    State(state): State<Arc<AppState>>,
    Json(tool_call): Json<ToolCall>,
) -> Result<Json<ObserveResponse>, ApiError> {
    if let Some(project) = tool_call.project.as_deref() {
        if ProjectFilter::global().is_some_and(|filter| filter.is_excluded(project)) {
            return Ok(Json(ObserveResponse { id: String::new(), queued: false }));
        }
    }

    // Filter injected memory tags BEFORE queuing to prevent re-observation recursion
    let tool_input = serde_json::to_string(&tool_call.input)
        .ok()
        .map(|s| filter_private_content(&filter_injected_memory(&s)));
    let session_id = tool_call.session_id.clone();
    let tool_name = tool_call.tool.clone();
    let tool_response = filter_private_content(&filter_injected_memory(&tool_call.output));
    let project = tool_call.project.clone();

    let message_id = state
        .queue_service
        .queue_message(
            &session_id,
            Some(&tool_name),
            tool_input.as_deref(),
            Some(&tool_response),
            project.as_deref(),
        )
        .await
        .map_err(|e| {
            tracing::error!("Queue message error: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        })?;

    Ok(Json(ObserveResponse { id: message_id.to_string(), queued: true }))
}

pub async fn observe_batch(
    State(state): State<Arc<AppState>>,
    Json(tool_calls): Json<Vec<ToolCall>>,
) -> Result<Json<ObserveBatchResponse>, ApiError> {
    let total = tool_calls.len();
    let mut count = 0usize;
    for tool_call in &tool_calls {
        if let Some(project) = tool_call.project.as_deref() {
            if ProjectFilter::global().is_some_and(|filter| filter.is_excluded(project)) {
                continue;
            }
        }
        let tool_input = serde_json::to_string(&tool_call.input)
            .ok()
            .map(|s| filter_private_content(&filter_injected_memory(&s)));
        let filtered_output = filter_private_content(&filter_injected_memory(&tool_call.output));
        match state
            .queue_service
            .queue_message(
                &tool_call.session_id,
                Some(&tool_call.tool),
                tool_input.as_deref(),
                Some(&filtered_output),
                tool_call.project.as_deref(),
            )
            .await
        {
            Ok(_id) => count = count.saturating_add(1),
            Err(e) => {
                tracing::error!("Failed to queue tool call {}: {}", tool_call.tool, e);
            },
        }
    }
    Ok(Json(ObserveBatchResponse { queued: count, total }))
}

pub async fn save_memory(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveMemoryRequest>,
) -> Result<(StatusCode, Json<Observation>), ApiError> {
    let text = req.text.trim();
    if text.is_empty() {
        return Err(ApiError::BadRequest("Bad Request".into()));
    }

    match state
        .observation_service
        .save_memory(text, req.title.as_deref(), req.project.as_deref())
        .await
    {
        Ok(opencode_mem_service::SaveMemoryResult::Created(obs)) => {
            Ok((StatusCode::CREATED, Json(obs)))
        },
        Ok(opencode_mem_service::SaveMemoryResult::Duplicate(obs)) => {
            Ok((StatusCode::OK, Json(obs)))
        },
        Ok(opencode_mem_service::SaveMemoryResult::Filtered) => {
            Err(ApiError::UnprocessableEntity("Unprocessable Entity".into()))
        },
        Err(e) => {
            tracing::error!("Save memory error: {}", e);
            Err(ApiError::Internal(anyhow::anyhow!("Internal Error")))
        },
    }
}

pub async fn get_observation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<Observation>>, ApiError> {
    state.search_service.get_observation_by_id(&id).await.map(Json).map_err(|e| {
        tracing::error!("Get observation error: {}", e);
        ApiError::Internal(anyhow::anyhow!("Internal Error"))
    })
}

pub async fn get_recent(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<Observation>>, ApiError> {
    state.search_service.get_recent_observations(query.capped_limit()).await.map(Json).map_err(
        |e| {
            tracing::error!("Get recent error: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        },
    )
}

pub async fn get_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    state
        .search_service
        .get_timeline(query.from.as_deref(), query.to.as_deref(), query.capped_limit())
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get timeline error: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        })
}

pub async fn get_observations_batch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchRequest>,
) -> Result<Json<Vec<Observation>>, ApiError> {
    if let Err(msg) = req.validate() {
        tracing::warn!("Batch request validation failed: {}", msg);
        return Err(ApiError::BadRequest("Bad Request".into()));
    }
    state.search_service.get_observations_by_ids(&req.ids).await.map(Json).map_err(|e| {
        tracing::error!("Get observations batch error: {}", e);
        ApiError::Internal(anyhow::anyhow!("Internal Error"))
    })
}

pub async fn get_observations_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<Observation>>, ApiError> {
    let limit = query.limit.min(opencode_mem_core::MAX_QUERY_LIMIT);
    state
        .search_service
        .get_observations_paginated(query.offset, limit, query.project.as_deref())
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get observations paginated error: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        })
}

pub async fn get_summaries_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<SessionSummary>>, ApiError> {
    let limit = query.limit.min(opencode_mem_core::MAX_QUERY_LIMIT);
    state
        .search_service
        .get_summaries_paginated(query.offset, limit, query.project.as_deref())
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get summaries paginated error: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        })
}

pub async fn get_prompts_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<UserPrompt>>, ApiError> {
    let limit = query.limit.min(opencode_mem_core::MAX_QUERY_LIMIT);
    state
        .search_service
        .get_prompts_paginated(query.offset, limit, query.project.as_deref())
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get prompts paginated error: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        })
}

pub async fn get_session_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<SessionSummary>>, ApiError> {
    state.search_service.get_session_summary(&id).await.map(Json).map_err(|e| {
        tracing::error!("Get session summary error: {}", e);
        ApiError::Internal(anyhow::anyhow!("Internal Error"))
    })
}

pub async fn get_prompt_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<UserPrompt>>, ApiError> {
    state.search_service.get_prompt_by_id(&id).await.map(Json).map_err(|e| {
        tracing::error!("Get prompt error: {}", e);
        ApiError::Internal(anyhow::anyhow!("Internal Error"))
    })
}
