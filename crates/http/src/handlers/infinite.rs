use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use opencode_mem_infinite::{StoredEvent, Summary};

use crate::api_types::{InfiniteTimeRangeQuery, SearchEntitiesQuery};
use crate::AppState;

pub async fn infinite_expand_summary(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<StoredEvent>>, (StatusCode, Json<serde_json::Value>)> {
    let infinite_mem = state.infinite_mem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "service_unavailable",
                "message": "Infinite Memory not configured (INFINITE_MEMORY_URL not set)"
            })),
        )
    })?;
    infinite_mem.get_events_by_summary_id(id, 1000).await.map(Json).map_err(|e| {
        tracing::error!("infinite_expand_summary failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal_error", "message": e.to_string()})),
        )
    })
}

pub async fn infinite_time_range(
    State(state): State<Arc<AppState>>,
    Query(query): Query<InfiniteTimeRangeQuery>,
) -> Result<Json<Vec<StoredEvent>>, (StatusCode, Json<serde_json::Value>)> {
    let infinite_mem = state.infinite_mem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "service_unavailable",
                "message": "Infinite Memory not configured (INFINITE_MEMORY_URL not set)"
            })),
        )
    })?;
    let start = chrono::DateTime::parse_from_rfc3339(&query.start)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid_start", "message": e.to_string()})),
            )
        })?
        .with_timezone(&chrono::Utc);
    let end = chrono::DateTime::parse_from_rfc3339(&query.end)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid_end", "message": e.to_string()})),
            )
        })?
        .with_timezone(&chrono::Utc);
    infinite_mem
        .get_events_by_time_range(start, end, query.session_id.as_deref(), 1000)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("infinite_time_range failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal_error", "message": e.to_string()})),
            )
        })
}

pub async fn infinite_drill_hour(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<Summary>>, (StatusCode, Json<serde_json::Value>)> {
    let infinite_mem = state.infinite_mem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "service_unavailable",
                "message": "Infinite Memory not configured (INFINITE_MEMORY_URL not set)"
            })),
        )
    })?;
    infinite_mem.get_5min_summaries_by_hour_id(id, 100).await.map(Json).map_err(|e| {
        tracing::error!("infinite_drill_hour failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal_error", "message": e.to_string()})),
        )
    })
}

pub async fn infinite_drill_day(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<Summary>>, (StatusCode, Json<serde_json::Value>)> {
    let infinite_mem = state.infinite_mem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "service_unavailable",
                "message": "Infinite Memory not configured (INFINITE_MEMORY_URL not set)"
            })),
        )
    })?;
    infinite_mem.get_hour_summaries_by_day_id(id, 100).await.map(Json).map_err(|e| {
        tracing::error!("infinite_drill_day failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal_error", "message": e.to_string()})),
        )
    })
}

pub async fn infinite_search_entities(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchEntitiesQuery>,
) -> Result<Json<Vec<Summary>>, (StatusCode, Json<serde_json::Value>)> {
    let infinite_mem = state.infinite_mem.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "service_unavailable",
                "message": "Infinite Memory not configured (INFINITE_MEMORY_URL not set)"
            })),
        )
    })?;

    infinite_mem
        .search_by_entity(&query.entity_type, &query.value, query.limit)
        .await
        .map(Json)
        .map_err(|e| {
            let error_msg = e.to_string();
            if error_msg.contains("Invalid entity_type") {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "invalid_entity_type",
                        "message": error_msg
                    })),
                )
            } else {
                tracing::error!("infinite_search_entities failed: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "internal_error", "message": error_msg})),
                )
            }
        })
}
