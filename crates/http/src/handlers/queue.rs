use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use std::env;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::spawn_blocking;

use opencode_mem_core::{ObservationInput, ProjectFilter, ToolOutput};
use opencode_mem_infinite::tool_event;
use opencode_mem_storage::{default_visibility_timeout_secs, PendingMessage};

use crate::api_types::{
    ClearQueueResponse, PendingQueueResponse, ProcessQueueResponse, ProcessingStatusResponse,
    SearchQuery, SetProcessingRequest, SetProcessingResponse,
};
use crate::blocking::blocking_result;
use crate::AppState;

use super::queue_processor::{max_queue_workers, process_pending_message};

pub async fn get_pending_queue(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<PendingQueueResponse>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    let limit = query.limit;
    let messages = blocking_result({
        let storage = Arc::clone(&storage);
        move || storage.get_all_pending_messages(limit)
    })
    .await?;
    let queue_stats = blocking_result(move || storage.get_queue_stats()).await?;
    Ok(Json(PendingQueueResponse { messages, stats: queue_stats }))
}

pub async fn process_pending_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProcessQueueResponse>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    let max_workers = max_queue_workers();
    let messages = blocking_result(move || {
        storage.claim_pending_messages(max_workers, default_visibility_timeout_secs())
    })
    .await?;

    if messages.is_empty() {
        return Ok(Json(ProcessQueueResponse { processed: 0, failed: 0 }));
    }

    let count = messages.len();
    let semaphore = Arc::new(Semaphore::new(max_workers));
    let mut handles = Vec::with_capacity(count);

    for msg in messages {
        let permit = Arc::clone(&semaphore).acquire_owned().await.map_err(|_sem_err| {
            tracing::error!("Semaphore closed unexpectedly");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        let state_clone = Arc::clone(&state);
        let handle = tokio::spawn(async move {
            let _permit = permit;
            let result = process_pending_message(&state_clone, &msg).await;
            match result {
                Ok(()) => {
                    let storage = Arc::clone(&state_clone.storage);
                    let msg_id = msg.id;
                    if let Err(e) = spawn_blocking(move || storage.complete_message(msg_id))
                        .await
                        .map_err(|e| anyhow::anyhow!("join error: {e}"))
                        .and_then(|r| r.map_err(|e| anyhow::anyhow!("{e}")))
                    {
                        tracing::error!("Complete message {} failed: {}", msg.id, e);
                        return false;
                    }
                    true
                },
                Err(e) => {
                    tracing::error!("Process message {} failed: {}", msg.id, e);
                    let storage = Arc::clone(&state_clone.storage);
                    let msg_id = msg.id;
                    if let Err(e) = spawn_blocking(move || storage.fail_message(msg_id, true))
                        .await
                        .map_err(|e| anyhow::anyhow!("join error: {e}"))
                        .and_then(|r| r.map_err(|e| anyhow::anyhow!("{e}")))
                    {
                        tracing::error!("Fail message {} error: {}", msg.id, e);
                    }
                    false
                },
            }
        });
        handles.push(handle);
    }

    let mut failed = 0usize;
    for handle in handles {
        match handle.await {
            Ok(true) => {},
            Ok(false) => failed = failed.saturating_add(1),
            Err(_join_err) => failed = failed.saturating_add(1),
        }
    }

    Ok(Json(ProcessQueueResponse { processed: count, failed }))
}

pub async fn clear_failed_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClearQueueResponse>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    let cleared = blocking_result(move || storage.clear_failed_messages()).await?;
    Ok(Json(ClearQueueResponse { cleared }))
}

pub async fn clear_all_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClearQueueResponse>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    let cleared = blocking_result(move || storage.clear_all_pending_messages()).await?;
    Ok(Json(ClearQueueResponse { cleared }))
}

pub async fn get_processing_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProcessingStatusResponse>, StatusCode> {
    let active = state.processing_active.load(Ordering::SeqCst);
    let storage = Arc::clone(&state.storage);
    let pending_count = blocking_result(move || storage.get_pending_count()).await?;
    Ok(Json(ProcessingStatusResponse { active, pending_count }))
}

pub async fn set_processing_status(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetProcessingRequest>,
) -> Result<Json<SetProcessingResponse>, StatusCode> {
    state.processing_active.store(req.active, Ordering::SeqCst);
    Ok(Json(SetProcessingResponse { active: req.active }))
}
