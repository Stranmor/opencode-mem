use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;

use opencode_mem_core::{Session, SessionStatus, ToolCall};

use crate::api_types::{
    SessionInitRequest, SessionInitResponse, SessionObservationsRequest,
    SessionObservationsResponse, SessionSummarizeRequest,
};
use crate::blocking::blocking_result;
use crate::AppState;

pub async fn api_session_init(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionInitRequest>,
) -> Result<Json<SessionInitResponse>, StatusCode> {
    let content_session_id = req.content_session_id.ok_or(StatusCode::BAD_REQUEST)?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let session = Session::new(
        session_id.clone(),
        content_session_id,
        None,
        req.project.unwrap_or_default(),
        req.user_prompt,
        chrono::Utc::now(),
        None,
        SessionStatus::Active,
        0,
    );
    state.session_service.init_session(session).map_err(|e| {
        tracing::error!("API session init failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(SessionInitResponse { session_id, status: "active".to_owned() }))
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
                },
            };
            let tool_call_with_session = tool_call.with_session_id(sid.clone());
            if let Err(e) = service.process(&id, tool_call_with_session).await {
                tracing::error!("Failed to process observation: {}", e);
            }
            drop(permit);
        });
    }
    Ok(Json(SessionObservationsResponse { queued: count, session_id }))
}

pub async fn api_session_summarize(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionSummarizeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let content_session_id = req.content_session_id.ok_or(StatusCode::BAD_REQUEST)?;

    let observations =
        state.session_service.summarize_session_from_export(&content_session_id).await.map_err(
            |e| {
                tracing::error!("Session summarize failed: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            },
        )?;

    let count = observations.len();
    Ok(Json(serde_json::json!({
        "content_session_id": content_session_id,
        "insights_extracted": count,
        "status": "completed"
    })))
}
