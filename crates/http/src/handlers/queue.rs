use crate::api_error::ApiError;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use opencode_mem_service::default_visibility_timeout_secs;

use crate::api_types::{
    ClearQueueResponse, PendingQueueResponse, ProcessQueueResponse, ProcessingStatusResponse,
    RetryQueueResponse, SearchQuery, SetProcessingRequest, SetProcessingResponse,
};
use crate::AppState;

use super::queue_processor::{max_queue_workers, process_pending_message};

pub async fn get_pending_queue(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<PendingQueueResponse>, crate::api_error::ApiError> {
    let messages = state
        .queue_service
        .get_all_pending_messages(query.capped_limit())
        .await
        .map_err(|e| crate::api_error::ApiError::Internal(anyhow::anyhow!(e)))?;
    let queue_stats = state
        .queue_service
        .get_queue_stats()
        .await
        .map_err(|e| crate::api_error::ApiError::Internal(anyhow::anyhow!(e)))?;
    Ok(Json(PendingQueueResponse { messages, stats: queue_stats }))
}

pub async fn process_pending_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProcessQueueResponse>, crate::api_error::ApiError> {
    let max_workers = max_queue_workers();
    let messages = state
        .queue_service
        .claim_pending_messages(max_workers, default_visibility_timeout_secs())
        .await
        .map_err(|e| crate::api_error::ApiError::Internal(anyhow::anyhow!(e)))?;

    if messages.is_empty() {
        return Ok(Json(ProcessQueueResponse { processed: 0, failed: 0 }));
    }

    let count = messages.len();
    let mut handles = Vec::with_capacity(count);

    for msg in messages {
        let permit = Arc::clone(&state.semaphore).acquire_owned().await.map_err(|_sem_err| {
            tracing::error!("Semaphore closed unexpectedly");
            crate::api_error::ApiError::Internal(anyhow::anyhow!("Internal Error"))
        })?;
        let state_clone = Arc::clone(&state);
        let handle = tokio::spawn(async move {
            let _permit = permit;
            let result = process_pending_message(&state_clone, &msg).await;
            match result {
                Ok(()) => {
                    if let Err(e) = state_clone.queue_service.complete_message(msg.id).await {
                        tracing::error!("Complete message {} failed: {}", msg.id, e);
                        return false;
                    }
                    true
                },
                Err(e) => {
                    tracing::error!("Process message {} failed: {}", msg.id, e);
                    if let Err(e) = state_clone.queue_service.fail_message(msg.id, true).await {
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
) -> Result<Json<ClearQueueResponse>, crate::api_error::ApiError> {
    let cleared = state
        .queue_service
        .clear_failed_messages()
        .await
        .map_err(|e| crate::api_error::ApiError::Internal(anyhow::anyhow!(e)))?;
    Ok(Json(ClearQueueResponse { cleared }))
}

pub async fn retry_failed_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RetryQueueResponse>, crate::api_error::ApiError> {
    let retried = state
        .queue_service
        .retry_failed_messages()
        .await
        .map_err(|e| crate::api_error::ApiError::Internal(anyhow::anyhow!(e)))?;
    Ok(Json(RetryQueueResponse { retried }))
}

pub async fn clear_all_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClearQueueResponse>, crate::api_error::ApiError> {
    let cleared = state
        .queue_service
        .clear_all_pending_messages()
        .await
        .map_err(|e| crate::api_error::ApiError::Internal(anyhow::anyhow!(e)))?;
    Ok(Json(ClearQueueResponse { cleared }))
}

pub async fn get_processing_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProcessingStatusResponse>, crate::api_error::ApiError> {
    let active = state.processing_active.load(Ordering::SeqCst);
    let pending_count = state
        .queue_service
        .get_pending_count()
        .await
        .map_err(|e| crate::api_error::ApiError::Internal(anyhow::anyhow!(e)))?;
    Ok(Json(ProcessingStatusResponse { active, pending_count }))
}

pub async fn set_processing_status(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetProcessingRequest>,
) -> Result<Json<SetProcessingResponse>, crate::api_error::ApiError> {
    state.processing_active.store(req.active, Ordering::SeqCst);
    Ok(Json(SetProcessingResponse { active: req.active }))
}
