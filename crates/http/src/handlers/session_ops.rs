use std::sync::Arc;

use axum::http::StatusCode;
use opencode_mem_core::{Session, SessionStatus, ToolCall};

use crate::api_types::{SessionInitResponse, SessionObservationsResponse};
use crate::AppState;

/// Create and persist a new session, returning the HTTP response.
///
/// Both legacy (path-based `session_db_id`) and API (generated UUID) handlers
/// provide their own `session_db_id` and `content_session_id` — this function
/// handles the shared `Session::new()` + `init_session()` logic.
pub(crate) async fn create_session(
    state: &Arc<AppState>,
    session_db_id: String,
    content_session_id: String,
    project: Option<String>,
    user_prompt: Option<String>,
) -> Result<SessionInitResponse, StatusCode> {
    let session = Session::new(
        session_db_id.clone(),
        content_session_id,
        None,
        project.unwrap_or_default(),
        user_prompt,
        chrono::Utc::now(),
        None,
        SessionStatus::Active,
        0,
    );
    state.session_service.init_session(session).await.map_err(|e| {
        tracing::error!("Session init failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(SessionInitResponse { session_id: session_db_id, status: "active".to_owned() })
}

/// Spawn observation processing tasks and return the queued count response.
///
/// Each observation gets a unique ID, acquires a semaphore permit, and is
/// processed asynchronously. Identical logic used by both legacy and API
/// observation endpoints.
pub(crate) fn spawn_observation_processing(
    state: &Arc<AppState>,
    session_id: String,
    observations: Vec<ToolCall>,
) -> SessionObservationsResponse {
    let count = observations.len();
    for tool_call in observations {
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
    SessionObservationsResponse { queued: count, session_id }
}
