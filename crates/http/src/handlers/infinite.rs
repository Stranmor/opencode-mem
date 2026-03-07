use crate::api_error::{ApiError, OrDegraded};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use std::sync::Arc;

use opencode_mem_core::INFINITE_MEMORY_NOT_CONFIGURED;

use opencode_mem_service::{InfiniteMemoryService, InfiniteSummary, StoredInfiniteEvent};

use crate::AppState;
use crate::api_types::{InfiniteTimeRangeQuery, SearchEntitiesQuery};

fn require_infinite_mem(
    state: &AppState,
) -> Result<&Arc<InfiniteMemoryService>, crate::api_error::ApiError> {
    state.infinite_mem.as_ref().ok_or_else(|| {
        crate::api_error::ApiError::ServiceUnavailable(INFINITE_MEMORY_NOT_CONFIGURED.to_string())
    })
}

pub async fn infinite_expand_summary(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<StoredInfiniteEvent>>, crate::api_error::ApiError> {
    let infinite_mem = require_infinite_mem(&state)?;
    infinite_mem
        .get_events_by_summary_id(id, opencode_mem_core::MAX_QUERY_LIMIT_I64)
        .await
        .map(Json)
        .map_err(|e| {
            if infinite_mem.circuit_breaker().is_open() {
                crate::api_error::ApiError::Degraded(serde_json::json!([]))
            } else {
                crate::api_error::ApiError::Internal(e.into())
            }
        })
}

pub async fn infinite_time_range(
    State(state): State<Arc<AppState>>,
    Query(query): Query<InfiniteTimeRangeQuery>,
) -> Result<Json<Vec<StoredInfiniteEvent>>, crate::api_error::ApiError> {
    let infinite_mem = require_infinite_mem(&state)?;
    let start = chrono::DateTime::parse_from_rfc3339(&query.start)
        .map_err(|e| crate::api_error::ApiError::BadRequest(format!("invalid_start: {}", e)))?
        .with_timezone(&chrono::Utc);
    let end = chrono::DateTime::parse_from_rfc3339(&query.end)
        .map_err(|e| crate::api_error::ApiError::BadRequest(format!("invalid_end: {}", e)))?
        .with_timezone(&chrono::Utc);
    infinite_mem
        .get_events_by_time_range(
            start,
            end,
            query.session_id.as_deref(),
            opencode_mem_core::MAX_QUERY_LIMIT_I64,
        )
        .await
        .map(Json)
        .map_err(|e| {
            if infinite_mem.circuit_breaker().is_open() {
                crate::api_error::ApiError::Degraded(serde_json::json!([]))
            } else {
                crate::api_error::ApiError::Internal(e.into())
            }
        })
}

pub async fn infinite_drill_hour(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<InfiniteSummary>>, crate::api_error::ApiError> {
    let infinite_mem = require_infinite_mem(&state)?;
    infinite_mem
        .get_5min_summaries_by_hour_id(id, opencode_mem_core::MAX_QUERY_LIMIT_I64)
        .await
        .map(Json)
        .map_err(|e| {
            if infinite_mem.circuit_breaker().is_open() {
                crate::api_error::ApiError::Degraded(serde_json::json!([]))
            } else {
                crate::api_error::ApiError::Internal(e.into())
            }
        })
}

pub async fn infinite_drill_day(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<InfiniteSummary>>, crate::api_error::ApiError> {
    let infinite_mem = require_infinite_mem(&state)?;
    infinite_mem
        .get_hour_summaries_by_day_id(id, opencode_mem_core::MAX_QUERY_LIMIT_I64)
        .await
        .map(Json)
        .map_err(|e| {
            if infinite_mem.circuit_breaker().is_open() {
                crate::api_error::ApiError::Degraded(serde_json::json!([]))
            } else {
                crate::api_error::ApiError::Internal(e.into())
            }
        })
}

pub async fn infinite_search_entities(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchEntitiesQuery>,
) -> Result<Json<Vec<InfiniteSummary>>, crate::api_error::ApiError> {
    let infinite_mem = require_infinite_mem(&state)?;

    infinite_mem
        .search_by_entity(&query.entity_type, &query.value, query.limit)
        .await
        .map(Json)
        .map_err(|e| {
            let error_msg = e.to_string();
            if error_msg.contains("Invalid entity_type") {
                crate::api_error::ApiError::BadRequest(error_msg)
            } else if infinite_mem.circuit_breaker().is_open() {
                crate::api_error::ApiError::Degraded(serde_json::json!([]))
            } else {
                crate::api_error::ApiError::Internal(e.into())
            }
        })
}
