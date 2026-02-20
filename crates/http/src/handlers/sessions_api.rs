use crate::api_error::ApiError;
use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;

use crate::api_types::{
    SessionInitRequest, SessionInitResponse, SessionObservationsRequest,
    SessionObservationsResponse, SessionSummarizeRequest,
};
use crate::AppState;

use super::session_ops::{create_session, enqueue_session_observations};

pub async fn api_session_init(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionInitRequest>,
) -> Result<Json<SessionInitResponse>, ApiError> {
    let content_session_id =
        req.content_session_id.ok_or(ApiError::BadRequest("Bad Request".into()))?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let resp = create_session(&state, session_id, content_session_id, req.project, req.user_prompt)
        .await?;
    Ok(Json(resp))
}

pub async fn api_session_observations(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionObservationsRequest>,
) -> Result<Json<SessionObservationsResponse>, ApiError> {
    let content_session_id =
        req.content_session_id.ok_or(ApiError::BadRequest("Bad Request".into()))?;
    let session =
        state.session_service.get_session_by_content_id(&content_session_id).await.map_err(
            |e| {
                tracing::error!("Get session by content id error: {}", e);
                ApiError::Internal(anyhow::anyhow!("Internal Error"))
            },
        )?;
    let session_id = session.map(|s| s.id).ok_or(ApiError::NotFound("Not Found".into()))?;
    let resp = enqueue_session_observations(&state, session_id, req.observations).await?;
    Ok(Json(resp))
}

pub async fn api_session_summarize(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionSummarizeRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let content_session_id =
        req.content_session_id.ok_or(ApiError::BadRequest("Bad Request".into()))?;

    // Look up session by content_session_id
    let session =
        state.session_service.get_session_by_content_id(&content_session_id).await.map_err(
            |e| {
                tracing::error!("Get session by content id error: {}", e);
                ApiError::Internal(anyhow::anyhow!("Internal Error"))
            },
        )?;
    let session_id = session.map(|s| s.id).ok_or_else(|| {
        tracing::warn!(content_session_id = %content_session_id, "Session not found for summarize");
        ApiError::NotFound("Not Found".into())
    })?;

    let cid = content_session_id.clone();
    let summary =
        state.session_service.summarize_session(&session_id, &cid).await.map_err(|e| {
            tracing::error!("Session summarize failed: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        })?;

    Ok(Json(serde_json::json!({
        "content_session_id": content_session_id,
        "session_id": session_id,
        "summary": summary,
        "status": "completed"
    })))
}
