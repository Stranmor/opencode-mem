use crate::api_error::ApiError;
use std::sync::Arc;

use opencode_mem_core::{Session, SessionStatus, ToolCall};

use crate::AppState;
use crate::api_types::{SessionInitResponse, SessionObservationsResponse};

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
) -> Result<SessionInitResponse, ApiError> {
    let session = Session::new(
        opencode_mem_core::SessionId(session_db_id.clone()),
        opencode_mem_core::ContentSessionId(content_session_id),
        None,
        opencode_mem_core::ProjectId(project.unwrap_or_default()),
        user_prompt,
        chrono::Utc::now(),
        None,
        SessionStatus::Active,
        0,
    );
    state
        .session_service
        .init_session(session)
        .await
        .map_err(|e| {
            tracing::error!("Session init failed: {}", e);
            ApiError::from(e)
        })?;
    Ok(SessionInitResponse {
        session_id: session_db_id,
        status: "active".to_owned(),
    })
}

/// Enqueue session observations into the persistent DB queue for durable processing.
///
/// Delegates to `QueueService::queue_tool_calls` which handles sanitization
/// and project exclusion filtering.
pub(crate) async fn enqueue_session_observations(
    state: &Arc<AppState>,
    session_id: String,
    observations: Vec<ToolCall>,
) -> Result<SessionObservationsResponse, ApiError> {
    let queued = state
        .queue_service
        .queue_tool_calls(&observations)
        .await
        .map_err(|e| {
            tracing::error!("Failed to queue session observations: {}", e);
            ApiError::from(e)
        })?;
    Ok(SessionObservationsResponse { queued, session_id })
}
