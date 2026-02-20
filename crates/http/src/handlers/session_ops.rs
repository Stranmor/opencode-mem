use crate::api_error::ApiError;
use std::sync::Arc;

use axum::http::StatusCode;
use opencode_mem_core::{filter_injected_memory, Session, SessionStatus, ToolCall};

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
) -> Result<SessionInitResponse, ApiError> {
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
        ApiError::Internal(anyhow::anyhow!("Internal Error"))
    })?;
    Ok(SessionInitResponse { session_id: session_db_id, status: "active".to_owned() })
}

/// Enqueue session observations into the persistent DB queue for durable processing.
///
/// Each observation is individually enqueued via `QueueService::queue_message`,
/// the same mechanism used by the `/observe` endpoint. The background queue
/// processor picks them up — no data loss on server crash.
pub(crate) async fn enqueue_session_observations(
    state: &Arc<AppState>,
    session_id: String,
    observations: Vec<ToolCall>,
) -> Result<SessionObservationsResponse, ApiError> {
    let mut queued = 0usize;
    for tool_call in &observations {
        let tool_input =
            serde_json::to_string(&tool_call.input).ok().map(|s| filter_injected_memory(&s));
        let filtered_output = filter_injected_memory(&tool_call.output);

        match state
            .queue_service
            .queue_message(
                &session_id,
                Some(&tool_call.tool),
                tool_input.as_deref(),
                Some(&filtered_output),
                tool_call.project.as_deref(),
            )
            .await
        {
            Ok(_id) => queued = queued.saturating_add(1),
            Err(e) => {
                tracing::error!("Failed to queue session observation {}: {}", tool_call.tool, e);
            },
        }
    }
    Ok(SessionObservationsResponse { queued, session_id })
}
