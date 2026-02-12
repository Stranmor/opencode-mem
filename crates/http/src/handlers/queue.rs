use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::Semaphore;

use opencode_mem_storage::{default_visibility_timeout_secs, PendingQueueStore};

use crate::api_types::{
    ClearQueueResponse, PendingQueueResponse, ProcessQueueResponse, ProcessingStatusResponse,
    RetryQueueResponse, SearchQuery, SetProcessingRequest, SetProcessingResponse,
};
use crate::AppState;

use super::queue_processor::{max_queue_workers, process_pending_message};

pub async fn get_pending_queue(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<PendingQueueResponse>, StatusCode> {
    let messages =
        state.storage.get_all_pending_messages(query.capped_limit()).await.map_err(|e| {
            tracing::error!("Get pending messages error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let queue_stats = state.storage.get_queue_stats().await.map_err(|e| {
        tracing::error!("Get queue stats error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(PendingQueueResponse { messages, stats: queue_stats }))
}

pub async fn process_pending_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProcessQueueResponse>, StatusCode> {
    let max_workers = max_queue_workers();
    let messages = state
        .storage
        .claim_pending_messages(max_workers, default_visibility_timeout_secs())
        .await
        .map_err(|e| {
            tracing::error!("Claim pending messages error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

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
                    if let Err(e) = state_clone.storage.complete_message(msg.id).await {
                        tracing::error!("Complete message {} failed: {}", msg.id, e);
                        return false;
                    }
                    true
                },
                Err(e) => {
                    tracing::error!("Process message {} failed: {}", msg.id, e);
                    if let Err(e) = state_clone.storage.fail_message(msg.id, true).await {
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
    let cleared = state.storage.clear_failed_messages().await.map_err(|e| {
        tracing::error!("Clear failed messages error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(ClearQueueResponse { cleared }))
}

pub async fn retry_failed_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RetryQueueResponse>, StatusCode> {
    let retried = state.storage.retry_failed_messages().await.map_err(|e| {
        tracing::error!("Retry failed messages error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(RetryQueueResponse { retried }))
}

pub async fn clear_all_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClearQueueResponse>, StatusCode> {
    let cleared = state.storage.clear_all_pending_messages().await.map_err(|e| {
        tracing::error!("Clear all pending messages error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(ClearQueueResponse { cleared }))
}

pub async fn get_processing_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProcessingStatusResponse>, StatusCode> {
    let active = state.processing_active.load(Ordering::SeqCst);
    let pending_count = state.storage.get_pending_count().await.map_err(|e| {
        tracing::error!("Get pending count error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(ProcessingStatusResponse { active, pending_count }))
}

pub async fn set_processing_status(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetProcessingRequest>,
) -> Result<Json<SetProcessingResponse>, StatusCode> {
    state.processing_active.store(req.active, Ordering::SeqCst);
    Ok(Json(SetProcessingResponse { active: req.active }))
}
