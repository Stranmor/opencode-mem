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

use opencode_mem_core::{ObservationInput, ToolOutput};
use opencode_mem_infinite::tool_event;
use opencode_mem_storage::{default_visibility_timeout_secs, PendingMessage};

use crate::api_types::{
    ClearQueueResponse, PendingQueueResponse, ProcessQueueResponse, ProcessingStatusResponse,
    SearchQuery, SetProcessingRequest, SetProcessingResponse,
};
use crate::blocking::blocking_result;
use crate::AppState;

fn max_queue_workers() -> usize {
    env::var("OPENCODE_MEM_QUEUE_WORKERS").ok().and_then(|v| v.parse().ok()).unwrap_or(10)
}

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
                    drop(spawn_blocking(move || storage.fail_message(msg_id, true)).await);
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

pub async fn process_pending_message(state: &AppState, msg: &PendingMessage) -> anyhow::Result<()> {
    let tool_name = msg.tool_name.as_deref().unwrap_or("unknown");
    let tool_input = msg.tool_input.clone();
    let tool_response = msg.tool_response.as_deref().unwrap_or("");

    let input = ObservationInput::new(
        tool_name.to_owned(),
        msg.session_id.clone(),
        String::new(),
        ToolOutput::new(
            format!("Observation from {tool_name}"),
            tool_response.to_owned(),
            tool_input
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(serde_json::Value::Null),
        ),
    );

    // Deterministic UUID v5 based on message ID to prevent duplicates
    // If same message is processed twice (race condition), same observation ID is generated
    let id =
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, msg.id.to_string().as_bytes()).to_string();
    let Some(observation) = state.llm.compress_to_observation(&id, &input, None).await? else {
        tracing::debug!("Observation filtered as trivial for message {}", msg.id);
        return Ok(());
    };
    let storage = Arc::clone(&state.storage);
    let obs_clone = observation.clone();
    spawn_blocking(move || storage.save_observation(&obs_clone))
        .await
        .map_err(|e| anyhow::anyhow!("save observation join error: {e}"))??;
    tracing::info!("Processed pending message {} -> observation {}", msg.id, observation.id);
    drop(state.event_tx.send(serde_json::to_string(&observation)?));

    if let Some(infinite_mem) = state.infinite_mem.as_ref() {
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

/// Releases stale messages back to pending queue on startup.
///
/// # Errors
/// Returns error if database operation fails.
pub fn run_startup_recovery(state: &AppState) -> anyhow::Result<usize> {
    let released = state.storage.release_stale_messages(default_visibility_timeout_secs())?;
    if released > 0 {
        tracing::info!("Startup recovery: released {} stale messages back to pending", released);
    }
    Ok(released)
}

/// Spawns background task that polls pending queue every 5 seconds.
pub fn start_background_processor(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            if !state.processing_active.load(Ordering::SeqCst) {
                continue;
            }
            tracing::debug!("Background processor: checking queue...");

            let storage = Arc::clone(&state.storage);
            let max_workers = max_queue_workers();
            let messages = match spawn_blocking(move || {
                storage.claim_pending_messages(max_workers, default_visibility_timeout_secs())
            })
            .await
            {
                Ok(Ok(msgs)) => msgs,
                Ok(Err(e)) => {
                    tracing::error!("Background processor: claim failed: {}", e);
                    continue;
                },
                Err(e) => {
                    tracing::error!("Background processor: join error: {}", e);
                    continue;
                },
            };

            if messages.is_empty() {
                continue;
            }

            let count = messages.len();
            let semaphore = Arc::new(Semaphore::new(max_workers));
            let mut handles = Vec::with_capacity(count);

            for msg in messages {
                let permit = match Arc::clone(&semaphore).acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let state_clone = Arc::clone(&state);
                let handle = tokio::spawn(async move {
                    let _permit = permit;
                    let result = process_pending_message(&state_clone, &msg).await;
                    match result {
                        Ok(()) => {
                            let storage = Arc::clone(&state_clone.storage);
                            let msg_id = msg.id;
                            drop(spawn_blocking(move || storage.complete_message(msg_id)).await);
                        },
                        Err(e) => {
                            tracing::error!("Background: process message {} failed: {}", msg.id, e);
                            let storage = Arc::clone(&state_clone.storage);
                            let msg_id = msg.id;
                            drop(spawn_blocking(move || storage.fail_message(msg_id, true)).await);
                        },
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                drop(handle.await);
            }

            tracing::info!("Background processor: processed {} messages", count);
        }
    });
}
