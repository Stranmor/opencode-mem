use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;

use opencode_mem_core::{Session, SessionStatus, ToolCall};

use crate::api_types::{
    SessionInitRequest, SessionInitResponse, SessionObservationsRequest,
    SessionObservationsResponse, SessionSummarizeRequest,
};
use crate::AppState;

pub async fn api_session_init(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionInitRequest>,
) -> Result<Json<SessionInitResponse>, StatusCode> {
    let content_session_id = req.content_session_id.ok_or(StatusCode::BAD_REQUEST)?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let session = Session {
        id: session_id.clone(),
        content_session_id,
        memory_session_id: None,
        project: req.project.unwrap_or_default(),
        user_prompt: req.user_prompt,
        started_at: chrono::Utc::now(),
        ended_at: None,
        status: SessionStatus::Active,
        prompt_counter: 0,
    };
    state.session_service.init_session(session).map_err(|e| {
        tracing::error!("API session init failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(SessionInitResponse {
        session_id,
        status: "active".to_string(),
    }))
}

pub async fn api_session_observations(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionObservationsRequest>,
) -> Result<Json<SessionObservationsResponse>, StatusCode> {
    let content_session_id = req.content_session_id.ok_or(StatusCode::BAD_REQUEST)?;
    let session = state
        .storage
        .get_session_by_content_id(&content_session_id)
        .map_err(|e| {
            tracing::error!("Get session by content id failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let session_id = session.map(|s| s.id).ok_or(StatusCode::NOT_FOUND)?;
    let count = req.observations.len();
    for tool_call in req.observations {
        let id = uuid::Uuid::new_v4().to_string();
        let service = state.observation_service.clone();
        let semaphore = state.semaphore.clone();
        let sid = session_id.clone();
        tokio::spawn(async move {
            let permit = match semaphore.acquire().await {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Semaphore closed: {}", e);
                    return;
                }
            };
            let tool_call_with_session = ToolCall {
                session_id: sid.clone(),
                ..tool_call
            };
            if let Err(e) = service.process(&id, tool_call_with_session).await {
                tracing::error!("Failed to process observation: {}", e);
            }
            drop(permit);
        });
    }
    Ok(Json(SessionObservationsResponse {
        queued: count,
        session_id,
    }))
}

pub async fn api_session_summarize(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionSummarizeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let content_session_id = req.content_session_id.ok_or(StatusCode::BAD_REQUEST)?;
    let session = state
        .storage
        .get_session_by_content_id(&content_session_id)
        .map_err(|e| {
            tracing::error!("Get session by content id failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let session_id = session.map(|s| s.id).ok_or(StatusCode::NOT_FOUND)?;
    let summary = state
        .session_service
        .summarize_session(&session_id)
        .await
        .map_err(|e| {
            tracing::error!("Generate summary failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(
        serde_json::json!({"session_id": session_id, "summary": summary, "queued": true}),
    ))
}
