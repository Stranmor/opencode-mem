use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::api_types::{
    SessionCompleteResponse, SessionDeleteResponse, SessionInitRequest, SessionInitResponse,
    SessionObservationsRequest, SessionObservationsResponse, SessionStatusResponse,
    SessionSummaryRequest,
};
use crate::blocking::blocking_result;
use crate::AppState;
use opencode_mem_core::SessionStatus;

use super::session_ops::{create_session, spawn_observation_processing};

pub async fn generate_summary(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionSummaryRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let summary = state.session_service.summarize_session(&req.session_id).await.map_err(|e| {
        tracing::error!("Generate summary failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(serde_json::json!({"session_id": req.session_id, "summary": summary})))
}

pub async fn session_init_legacy(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
    Json(req): Json<SessionInitRequest>,
) -> Result<Json<SessionInitResponse>, StatusCode> {
    let content_session_id = req.content_session_id.unwrap_or_else(|| session_db_id.clone());
    let resp =
        create_session(&state, session_db_id, content_session_id, req.project, req.user_prompt)?;
    Ok(Json(resp))
}

pub async fn session_observations_legacy(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
    Json(req): Json<SessionObservationsRequest>,
) -> Result<Json<SessionObservationsResponse>, StatusCode> {
    let resp = spawn_observation_processing(&state, session_db_id, req.observations);
    Ok(Json(resp))
}

pub async fn session_summarize_legacy(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let summary = state.session_service.summarize_session(&session_db_id).await.map_err(|e| {
        tracing::error!("Generate summary failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(serde_json::json!({"session_id": session_db_id, "summary": summary, "queued": true})))
}

pub async fn session_status(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<SessionStatusResponse>, StatusCode> {
    let storage = state.storage.clone();
    let id = session_db_id.clone();
    let session = blocking_result(move || storage.get_session(&id)).await?;
    match session {
        Some(s) => {
            let storage = state.storage.clone();
            let id = session_db_id.clone();
            let obs_count = blocking_result(move || storage.get_session_observation_count(&id))
                .await
                .unwrap_or(0);
            Ok(Json(SessionStatusResponse {
                session_id: s.id,
                status: s.status,
                observation_count: obs_count,
                started_at: s.started_at.to_rfc3339(),
                ended_at: s.ended_at.map(|d| d.to_rfc3339()),
            }))
        },
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn session_delete(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<SessionDeleteResponse>, StatusCode> {
    let storage = state.storage.clone();
    let id = session_db_id.clone();
    let deleted = blocking_result(move || storage.delete_session(&id)).await?;
    Ok(Json(SessionDeleteResponse { deleted, session_id: session_db_id }))
}

pub async fn session_complete(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<SessionCompleteResponse>, StatusCode> {
    let summary = state.session_service.complete_session(&session_db_id).await.map_err(|e| {
        tracing::error!("Complete session failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(SessionCompleteResponse {
        session_id: session_db_id,
        status: SessionStatus::Completed,
        summary,
    }))
}
