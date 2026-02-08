use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;

use crate::api_types::{
    SessionInitRequest, SessionInitResponse, SessionObservationsRequest,
    SessionObservationsResponse, SessionSummarizeRequest,
};
use crate::blocking::blocking_result;
use crate::AppState;

use super::session_ops::{create_session, spawn_observation_processing};

pub async fn api_session_init(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionInitRequest>,
) -> Result<Json<SessionInitResponse>, StatusCode> {
    let content_session_id = req.content_session_id.ok_or(StatusCode::BAD_REQUEST)?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let resp =
        create_session(&state, session_id, content_session_id, req.project, req.user_prompt)?;
    Ok(Json(resp))
}

pub async fn api_session_observations(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionObservationsRequest>,
) -> Result<Json<SessionObservationsResponse>, StatusCode> {
    let content_session_id = req.content_session_id.ok_or(StatusCode::BAD_REQUEST)?;
    let storage = state.storage.clone();
    let cid = content_session_id.clone();
    let session = blocking_result(move || storage.get_session_by_content_id(&cid)).await?;
    let session_id = session.map(|s| s.id).ok_or(StatusCode::NOT_FOUND)?;
    let resp = spawn_observation_processing(&state, session_id, req.observations);
    Ok(Json(resp))
}

pub async fn api_session_summarize(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionSummarizeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let content_session_id = req.content_session_id.ok_or(StatusCode::BAD_REQUEST)?;

    // Look up session by content_session_id
    let storage = state.storage.clone();
    let cid = content_session_id.clone();
    let session = blocking_result(move || storage.get_session_by_content_id(&cid)).await?;
    let session_id = session.map(|s| s.id).ok_or_else(|| {
        tracing::warn!(content_session_id = %content_session_id, "Session not found for summarize");
        StatusCode::NOT_FOUND
    })?;

    // Generate summary from observations already in SQLite
    let cid = content_session_id.clone();
    let summary =
        state.session_service.summarize_session(&session_id, &cid).await.map_err(|e| {
            tracing::error!("Session summarize failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!({
        "content_session_id": content_session_id,
        "session_id": session_id,
        "summary": summary,
        "status": "completed"
    })))
}
