use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::Semaphore;

use opencode_mem_core::{ObservationInput, ToolOutput};
use opencode_mem_infinite::tool_event;
use opencode_mem_storage::{PendingMessage, DEFAULT_VISIBILITY_TIMEOUT_SECS};

use crate::api_types::{
    ClearQueueResponse, PendingQueueResponse, ProcessQueueResponse, ProcessingStatusResponse,
    SearchQuery, SetProcessingRequest, SetProcessingResponse,
};
use crate::AppState;

/// Maximum number of concurrent workers for queue processing.
/// This bounds memory usage and prevents visibility timeout race conditions.
const MAX_QUEUE_WORKERS: usize = 10;

pub async fn get_pending_queue(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<PendingQueueResponse>, StatusCode> {
    let messages = state
        .storage
        .get_all_pending_messages(query.limit)
        .map_err(|e| {
            tracing::error!("Get pending queue failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let stats = state.storage.get_queue_stats().map_err(|e| {
        tracing::error!("Get queue stats failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(PendingQueueResponse { messages, stats }))
}

pub async fn process_pending_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProcessQueueResponse>, StatusCode> {
    let messages = state
        .storage
        .claim_pending_messages(MAX_QUEUE_WORKERS, DEFAULT_VISIBILITY_TIMEOUT_SECS)
        .map_err(|e| {
            tracing::error!("Claim pending messages failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if messages.is_empty() {
        return Ok(Json(ProcessQueueResponse {
            processed: 0,
            failed: 0,
        }));
    }

    let count = messages.len();
    let semaphore = Arc::new(Semaphore::new(MAX_QUEUE_WORKERS));
    let mut handles = Vec::with_capacity(count);

    for msg in messages {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed unexpectedly");
        let state_clone = state.clone();
        let handle = tokio::spawn(async move {
            let _permit = permit; // Hold permit until task completes
            let result = process_pending_message(&state_clone, &msg).await;
            match result {
                Ok(()) => {
                    if let Err(e) = state_clone.storage.complete_message(msg.id) {
                        tracing::error!("Complete message {} failed: {}", msg.id, e);
                        return false;
                    }
                    true
                }
                Err(e) => {
                    tracing::error!("Process message {} failed: {}", msg.id, e);
                    let _ = state_clone.storage.fail_message(msg.id, true);
                    false
                }
            }
        });
        handles.push(handle);
    }

    let mut failed = 0usize;
    for handle in handles {
        if let Ok(success) = handle.await {
            if !success {
                failed += 1;
            }
        } else {
            failed += 1;
        }
    }

    Ok(Json(ProcessQueueResponse {
        processed: count,
        failed,
    }))
}

pub async fn process_pending_message(state: &AppState, msg: &PendingMessage) -> anyhow::Result<()> {
    let tool_name = msg.tool_name.as_deref().unwrap_or("unknown");
    let tool_input = msg.tool_input.clone();
    let tool_response = msg.tool_response.as_deref().unwrap_or("");

    let input = ObservationInput {
        tool: tool_name.to_string(),
        session_id: msg.session_id.clone(),
        call_id: String::new(),
        output: ToolOutput {
            title: format!("Observation from {}", tool_name),
            output: tool_response.to_string(),
            metadata: tool_input
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(serde_json::Value::Null),
        },
    };

    let id = uuid::Uuid::new_v4().to_string();
    let observation = state.llm.compress_to_observation(&id, &input, None).await?;
    state.storage.save_observation(&observation)?;
    tracing::info!(
        "Processed pending message {} -> observation {}",
        msg.id,
        observation.id
    );
    let _ = state.event_tx.send(serde_json::to_string(&observation)?);

    if let Some(ref infinite_mem) = state.infinite_mem {
        let event = tool_event(
            &msg.session_id,
            None,
            tool_name,
            tool_input
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(serde_json::Value::Null),
            serde_json::json!({"output": tool_response}),
            observation.files_modified.clone(),
        );
        if let Err(e) = infinite_mem.store_event(event).await {
            tracing::warn!("Failed to store in infinite memory: {}", e);
        }
    }

    Ok(())
}

pub async fn clear_failed_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClearQueueResponse>, StatusCode> {
    let cleared = state.storage.clear_failed_messages().map_err(|e| {
        tracing::error!("Clear failed messages failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(ClearQueueResponse { cleared }))
}

pub async fn clear_all_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClearQueueResponse>, StatusCode> {
    let cleared = state.storage.clear_all_pending_messages().map_err(|e| {
        tracing::error!("Clear all pending messages failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(ClearQueueResponse { cleared }))
}

pub async fn get_processing_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProcessingStatusResponse>, StatusCode> {
    let active = state.processing_active.load(Ordering::SeqCst);
    let pending_count = state.storage.get_pending_count().map_err(|e| {
        tracing::error!("Get pending count failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(ProcessingStatusResponse {
        active,
        pending_count,
    }))
}

pub async fn set_processing_status(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetProcessingRequest>,
) -> Result<Json<SetProcessingResponse>, StatusCode> {
    state.processing_active.store(req.active, Ordering::SeqCst);
    Ok(Json(SetProcessingResponse { active: req.active }))
}

pub fn run_startup_recovery(state: &AppState) -> anyhow::Result<usize> {
    let released = state
        .storage
        .release_stale_messages(DEFAULT_VISIBILITY_TIMEOUT_SECS)?;
    if released > 0 {
        tracing::info!(
            "Startup recovery: released {} stale messages back to pending",
            released
        );
    }
    Ok(released)
}
