use crate::api_error::{ApiError, DegradedExt, OrDegraded};
use axum::{
    Json,
    extract::{Path, State},
};
use serde_json::json;
use std::sync::Arc;

use crate::AppState;
use crate::api_types::{
    SessionCompleteResponse, SessionDeleteResponse, SessionInitRequest, SessionInitResponse,
    SessionObservationsRequest, SessionObservationsResponse, SessionStatusResponse,
    SessionSummaryRequest,
};
use opencode_mem_core::SessionStatus;

use super::session_ops::{create_session, enqueue_session_observations};

pub async fn generate_summary(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionSummaryRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Legacy API: session_id serves as both UUID and content_session_id
    let summary = state
        .session_service
        .summarize_session(&req.session_id, &req.session_id)
        .await
        .map_err(|e| {
            tracing::error!("Generate summary failed: {}", e);
            ApiError::from(e)
        })
        .with_degraded_body(json!({
            "session_id": req.session_id,
            "summary": "Database unavailable, summary skipped."
        }))?;
    Ok(Json(
        serde_json::json!({"session_id": req.session_id, "summary": summary}),
    ))
}

pub async fn session_init_legacy(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
    Json(req): Json<SessionInitRequest>,
) -> Result<Json<SessionInitResponse>, ApiError> {
    let content_session_id = req
        .content_session_id
        .unwrap_or_else(|| session_db_id.clone());
    let resp = create_session(
        &state,
        session_db_id.clone(),
        content_session_id,
        req.project,
        req.user_prompt,
    )
    .await
    .with_degraded_body(json!({
        "session_id": session_db_id,
        "status": "active"
    }))?;
    Ok(Json(resp))
}

pub async fn session_observations_legacy(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
    Json(req): Json<SessionObservationsRequest>,
) -> Result<Json<SessionObservationsResponse>, ApiError> {
    let resp = enqueue_session_observations(&state, session_db_id.clone(), req.observations)
        .await
        .with_degraded_body(json!({
            "queued": 0,
            "session_id": session_db_id
        }))?;
    Ok(Json(resp))
}

pub async fn session_summarize_legacy(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Legacy API: session_db_id serves as both UUID and content_session_id
    let summary = state
        .session_service
        .summarize_session(&session_db_id, &session_db_id)
        .await
        .map_err(|e| {
            tracing::error!("Generate summary failed: {}", e);
            ApiError::from(e)
        })
        .with_degraded_body(json!({
            "session_id": session_db_id,
            "summary": "Database unavailable, summary skipped.",
            "queued": true
        }))?;
    Ok(Json(
        serde_json::json!({"session_id": session_db_id, "summary": summary, "queued": true}),
    ))
}

pub async fn session_status(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<SessionStatusResponse>, ApiError> {
    let session = state
        .session_service
        .get_session(&session_db_id)
        .await
        .map_err(ApiError::from)
        .with_degraded_body(json!({
            "session_id": session_db_id,
            "status": "active",
            "observation_count": 0,
            "started_at": chrono::Utc::now().to_rfc3339()
        }))?;

    match session {
        Some(s) => {
            let obs_count = state
                .session_service
                .get_session_observation_count(&session_db_id)
                .await
                .unwrap_or(0);
            Ok(Json(SessionStatusResponse {
                session_id: s.id.to_string(),
                status: s.status,
                observation_count: obs_count,
                started_at: s.started_at.to_rfc3339(),
                ended_at: s.ended_at.map(|d| d.to_rfc3339()),
            }))
        }
        None => Err(ApiError::NotFound("Not Found".into())),
    }
}

pub async fn session_delete(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<SessionDeleteResponse>, ApiError> {
    let deleted = state
        .session_service
        .delete_session(&session_db_id)
        .await
        .map_err(|e| {
            tracing::error!("Delete session error: {}", e);
            ApiError::from(e)
        })?;
    Ok(Json(SessionDeleteResponse {
        deleted,
        session_id: session_db_id,
    }))
}

pub async fn session_complete(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<SessionCompleteResponse>, ApiError> {
    let summary = state
        .session_service
        .complete_session(&session_db_id)
        .await
        .map_err(|e| {
            tracing::error!("Complete session failed: {}", e);
            ApiError::from(e)
        })
        .with_degraded_body(json!({
            "session_id": session_db_id,
            "status": "completed",
            "summary": "Database unavailable, summary skipped."
        }))?;
    Ok(Json(SessionCompleteResponse {
        session_id: session_db_id,
        status: SessionStatus::Completed,
        summary,
    }))
}
