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
    let storage = state.storage.clone();
    let limit = query.limit;
    let messages = tokio::task::spawn_blocking({
        let storage = storage.clone();
        move || storage.get_all_pending_messages(limit)
    })
    .await
    .map_err(|e| {
        tracing::error!("Get pending queue join error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .map_err(|e| {
        tracing::error!("Get pending queue failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let stats = tokio::task::spawn_blocking(move || storage.get_queue_stats())
        .await
        .map_err(|e| {
            tracing::error!("Get queue stats join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map_err(|e| {
            tracing::error!("Get queue stats failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(PendingQueueResponse { messages, stats }))
}

pub async fn process_pending_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProcessQueueResponse>, StatusCode> {
    let storage = state.storage.clone();
    let messages = tokio::task::spawn_blocking(move || {
        storage.claim_pending_messages(MAX_QUEUE_WORKERS, DEFAULT_VISIBILITY_TIMEOUT_SECS)
    })
    .await
    .map_err(|e| {
        tracing::error!("Claim pending messages join error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
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
                    let storage = state_clone.storage.clone();
                    let msg_id = msg.id;
                    if let Err(e) = tokio::task::spawn_blocking(move || {
                        storage.complete_message(msg_id)
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("join error: {}", e))
                    .and_then(|r| r.map_err(|e| anyhow::anyhow!("{}", e)))
                    {
                        tracing::error!("Complete message {} failed: {}", msg.id, e);
                        return false;
                    }
                    true
                }
                Err(e) => {
                    tracing::error!("Process message {} failed: {}", msg.id, e);
                    let storage = state_clone.storage.clone();
                    let msg_id = msg.id;
                    let _ = tokio::task::spawn_blocking(move || {
                        storage.fail_message(msg_id, true)
                    })
                    .await;
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
    let storage = state.storage.clone();
    let obs_clone = observation.clone();
    tokio::task::spawn_blocking(move || storage.save_observation(&obs_clone))
        .await
        .map_err(|e| anyhow::anyhow!("save observation join error: {}", e))??;
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
    let storage = state.storage.clone();
    let cleared = tokio::task::spawn_blocking(move || storage.clear_failed_messages())
        .await
        .map_err(|e| {
            tracing::error!("Clear failed messages join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map_err(|e| {
            tracing::error!("Clear failed messages failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(ClearQueueResponse { cleared }))
}

pub async fn clear_all_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClearQueueResponse>, StatusCode> {
    let storage = state.storage.clone();
    let cleared = tokio::task::spawn_blocking(move || storage.clear_all_pending_messages())
        .await
        .map_err(|e| {
            tracing::error!("Clear all pending messages join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map_err(|e| {
            tracing::error!("Clear all pending messages failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(ClearQueueResponse { cleared }))
}

pub async fn get_processing_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProcessingStatusResponse>, StatusCode> {
    let active = state.processing_active.load(Ordering::SeqCst);
    let storage = state.storage.clone();
    let pending_count = tokio::task::spawn_blocking(move || storage.get_pending_count())
        .await
        .map_err(|e| {
            tracing::error!("Get pending count join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map_err(|e| {
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
