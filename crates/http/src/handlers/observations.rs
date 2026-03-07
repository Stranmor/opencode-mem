use super::is_localhost;
use crate::api_error::{ApiError, DegradedExt, OrDegraded};
use axum::{
    Json,
    extract::{ConnectInfo, Path, Query, State},
    http::StatusCode,
};
use serde_json::json;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use opencode_mem_core::{
    NoiseLevel, Observation, ObservationType, SearchResult, SessionSummary, ToolCall, UserPrompt,
};
use opencode_mem_service::{PaginatedResult, QueueToolCallResult};

use crate::AppState;
use crate::api_types::{
    BatchRequest, ObserveBatchResponse, ObserveResponse, PaginationQuery, SaveMemoryRequest,
    SearchQuery, TimelineQuery,
};

pub async fn observe(
    State(state): State<Arc<AppState>>,
    Json(tool_call): Json<ToolCall>,
) -> Result<Json<ObserveResponse>, ApiError> {
    match state
        .queue_service
        .queue_tool_call(&tool_call)
        .await
        .map_err(|e| {
            tracing::error!("Queue message error: {}", e);
            ApiError::from(e)
        })
        .with_degraded_body(json!({ "id": "", "queued": false }))?
    {
        QueueToolCallResult::Queued(id) => Ok(Json(ObserveResponse {
            id: id.to_string(),
            queued: true,
        })),
        QueueToolCallResult::ExcludedProject => Ok(Json(ObserveResponse {
            id: String::new(),
            queued: false,
        })),
    }
}

pub async fn observe_batch(
    State(state): State<Arc<AppState>>,
    Json(tool_calls): Json<Vec<ToolCall>>,
) -> Result<Json<ObserveBatchResponse>, ApiError> {
    let total = tool_calls.len();
    let count = state
        .queue_service
        .queue_tool_calls(&tool_calls)
        .await
        .map_err(|e| {
            tracing::error!("Queue batch error: {}", e);
            ApiError::from(e)
        })
        .with_degraded_body(json!({ "queued": 0, "total": total }))?;
    Ok(Json(ObserveBatchResponse {
        queued: count,
        total,
    }))
}

pub async fn save_memory(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveMemoryRequest>,
) -> Result<(StatusCode, Json<Observation>), ApiError> {
    let text = req.text.trim();
    if text.is_empty() {
        return Err(ApiError::BadRequest("Bad Request".into()));
    }

    let title_str = match req.title.as_deref() {
        Some(t) if !t.trim().is_empty() => opencode_mem_core::sanitize_input(t.trim()),
        _ => text.chars().take(50).collect::<String>(),
    };

    let observation_type = match req
        .observation_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => ObservationType::from_str(raw).map(Some).map_err(|_| {
            ApiError::BadRequest(format!(
                "invalid observation_type: {raw} (allowed: {})",
                ObservationType::ALL_VARIANTS_STR
            ))
        })?,
        None => None,
    };
    let noise_level = match req
        .noise_level
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => NoiseLevel::from_str(raw).map(Some).map_err(|_| {
            ApiError::BadRequest(format!(
                "invalid noise_level: {raw} (allowed: {})",
                NoiseLevel::ALL_VARIANTS_STR
            ))
        })?,
        None => None,
    };

    match state
        .observation_service
        .save_memory(
            text,
            Some(&title_str),
            req.project.as_deref(),
            observation_type,
            noise_level,
        )
        .await
        .map_err(|e| {
            if e.is_db_unavailable() || e.is_transient() {
                // Buffer the write in the pending queue for later flush
                state.queue_service.push_pending_write(
                    opencode_mem_service::PendingWrite::SaveMemory {
                        text: text.to_owned(),
                        title: Some(title_str.clone()),
                        project: req.project.clone(),
                        observation_type,
                        noise_level,
                    },
                );
            }
            tracing::error!("Save memory error: {}", e);
            ApiError::from(e)
        })
        .with_degraded_body(json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "session_id": "manual",
            "title": title_str,
            "observation_type": "Discovery",
            "noise_level": "Medium",
            "created_at": chrono::Utc::now().to_rfc3339()
        }))? {
        opencode_mem_service::SaveMemoryResult::Created(obs) => {
            Ok((StatusCode::CREATED, Json(obs)))
        }
        opencode_mem_service::SaveMemoryResult::Duplicate(obs) => Ok((StatusCode::OK, Json(obs))),
        opencode_mem_service::SaveMemoryResult::Filtered => {
            Err(ApiError::UnprocessableEntity("Unprocessable Entity".into()))
        }
    }
}

pub async fn get_observation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<Observation>>, ApiError> {
    state
        .search_service
        .get_observation_by_id(&id)
        .await
        .or_degraded(None::<Observation>)
        .map(Json)
}

pub async fn get_recent(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<Observation>>, ApiError> {
    state
        .search_service
        .get_recent_observations(query.capped_limit())
        .await
        .or_degraded(Vec::<Observation>::new())
        .map(Json)
}

pub async fn get_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    state
        .search_service
        .get_timeline(
            query.from.as_deref(),
            query.to.as_deref(),
            query.capped_limit(),
        )
        .await
        .or_degraded(Vec::<SearchResult>::new())
        .map(Json)
}

pub async fn get_observations_batch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchRequest>,
) -> Result<Json<Vec<Observation>>, ApiError> {
    if let Err(msg) = req.validate() {
        tracing::warn!("Batch request validation failed: {}", msg);
        return Err(ApiError::BadRequest("Bad Request".into()));
    }
    state
        .search_service
        .get_observations_by_ids(&req.ids)
        .await
        .or_degraded(Vec::<Observation>::new())
        .map(Json)
}

pub async fn get_observations_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<Observation>>, ApiError> {
    state
        .search_service
        .get_observations_paginated(query.offset, query.capped_limit(), query.project.as_deref())
        .await
        .or_degraded(PaginatedResult::<Observation>::empty())
        .map(Json)
}

pub async fn get_summaries_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<SessionSummary>>, ApiError> {
    state
        .search_service
        .get_summaries_paginated(query.offset, query.capped_limit(), query.project.as_deref())
        .await
        .or_degraded(PaginatedResult::<SessionSummary>::empty())
        .map(Json)
}

pub async fn get_prompts_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<UserPrompt>>, ApiError> {
    state
        .search_service
        .get_prompts_paginated(query.offset, query.capped_limit(), query.project.as_deref())
        .await
        .or_degraded(PaginatedResult::<UserPrompt>::empty())
        .map(Json)
}

pub async fn get_session_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<SessionSummary>>, ApiError> {
    state
        .search_service
        .get_session_summary(&id)
        .await
        .or_degraded(None::<SessionSummary>)
        .map(Json)
}

pub async fn get_prompt_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<UserPrompt>>, ApiError> {
    state
        .search_service
        .get_prompt_by_id(&id)
        .await
        .or_degraded(None::<UserPrompt>)
        .map(Json)
}

pub async fn delete_observation(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    axum::http::HeaderMap(headers): axum::http::HeaderMap,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !super::check_admin_access(&addr, &headers, &state.config) {
        return Err(ApiError::Forbidden("Forbidden".into()));
    }
    let deleted = state
        .observation_service
        .delete_observation(&id)
        .await
        .map_err(|e| {
            tracing::error!("Delete observation error: {}", e);
            ApiError::from(e)
        })?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!("observation '{id}' not found")))
    }
}
