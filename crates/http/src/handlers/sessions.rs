use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use opencode_mem_core::{Session, SessionStatus, ToolCall};

use crate::api_types::{
    SessionCompleteResponse, SessionDeleteResponse, SessionInitRequest, SessionInitResponse,
    SessionObservationsRequest, SessionObservationsResponse, SessionStatusResponse,
    SessionSummaryRequest,
};
use crate::AppState;

pub async fn generate_summary(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionSummaryRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let summary = state
        .session_service
        .summarize_session(&req.session_id)
        .await
        .map_err(|e| {
            tracing::error!("Generate summary failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(
        serde_json::json!({"session_id": req.session_id, "summary": summary}),
    ))
}

pub async fn session_init_legacy(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
    Json(req): Json<SessionInitRequest>,
) -> Result<Json<SessionInitResponse>, StatusCode> {
    let session = Session {
        id: session_db_id.clone(),
        content_session_id: req
            .content_session_id
            .unwrap_or_else(|| session_db_id.clone()),
        memory_session_id: None,
        project: req.project.unwrap_or_default(),
        user_prompt: req.user_prompt,
        started_at: chrono::Utc::now(),
        ended_at: None,
        status: SessionStatus::Active,
        prompt_counter: 0,
    };
    state
        .session_service
        .init_session(session.clone())
        .map_err(|e| {
            tracing::error!("Session init failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(SessionInitResponse {
        session_id: session.id,
        status: "active".to_string(),
    }))
}

pub async fn session_observations_legacy(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
    Json(req): Json<SessionObservationsRequest>,
) -> Result<Json<SessionObservationsResponse>, StatusCode> {
    let count = req.observations.len();
    for tool_call in req.observations {
        let id = uuid::Uuid::new_v4().to_string();
        let service = state.observation_service.clone();
        let semaphore = state.semaphore.clone();
        let session_id = session_db_id.clone();
        tokio::spawn(async move {
            let permit = match semaphore.acquire().await {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Semaphore closed: {}", e);
                    return;
                }
            };
            let tool_call_with_session = ToolCall {
                session_id: session_id.clone(),
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
        session_id: session_db_id,
    }))
}

pub async fn session_summarize_legacy(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let summary = state
        .session_service
        .summarize_session(&session_db_id)
        .await
        .map_err(|e| {
            tracing::error!("Generate summary failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(
        serde_json::json!({"session_id": session_db_id, "summary": summary, "queued": true}),
    ))
}

pub async fn session_status(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<SessionStatusResponse>, StatusCode> {
    let storage = state.storage.clone();
    let id = session_db_id.clone();
    let session = tokio::task::spawn_blocking(move || storage.get_session(&id))
        .await
        .map_err(|e| {
            tracing::error!("Get session join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map_err(|e| {
            tracing::error!("Get session failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    match session {
        Some(s) => {
            let storage = state.storage.clone();
            let id = session_db_id.clone();
            let obs_count = tokio::task::spawn_blocking(move || {
                storage.get_session_observation_count(&id)
            })
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or(0);
            Ok(Json(SessionStatusResponse {
                session_id: s.id,
                status: s.status,
                observation_count: obs_count,
                started_at: s.started_at.to_rfc3339(),
                ended_at: s.ended_at.map(|d| d.to_rfc3339()),
            }))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn session_delete(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<SessionDeleteResponse>, StatusCode> {
    let storage = state.storage.clone();
    let id = session_db_id.clone();
    let deleted = tokio::task::spawn_blocking(move || storage.delete_session(&id))
        .await
        .map_err(|e| {
            tracing::error!("Delete session join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map_err(|e| {
            tracing::error!("Delete session failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(SessionDeleteResponse {
        deleted,
        session_id: session_db_id,
    }))
}

pub async fn session_complete(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<SessionCompleteResponse>, StatusCode> {
    let summary = state
        .session_service
        .complete_session(&session_db_id)
        .await
        .map_err(|e| {
            tracing::error!("Complete session failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(SessionCompleteResponse {
        session_id: session_db_id,
        status: SessionStatus::Completed,
        summary,
    }))
}
