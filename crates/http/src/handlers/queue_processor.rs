use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::Semaphore;

use opencode_mem_core::{ProjectFilter, ToolCall};
use opencode_mem_service::{default_visibility_timeout_secs, PendingMessage};

use crate::AppState;

pub(crate) fn max_queue_workers() -> usize {
    opencode_mem_core::env_parse_with_default("OPENCODE_MEM_QUEUE_WORKERS", 10)
}

pub async fn process_pending_message(state: &AppState, msg: &PendingMessage) -> anyhow::Result<()> {
    if let Some(project) = msg.project.as_deref().filter(|p| !p.is_empty() && *p != "unknown") {
        if ProjectFilter::global().is_some_and(|filter| filter.is_excluded(project)) {
            tracing::debug!("Skipping excluded project '{}' for message {}", project, msg.id);
            return Ok(());
        }
    }

    let tool_name = msg.tool_name.as_deref().unwrap_or("unknown");
    let tool_input: serde_json::Value = msg
        .tool_input
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(serde_json::Value::Null);
    let tool_response = msg.tool_response.as_deref().unwrap_or("");

    let tool_call = ToolCall::new(
        tool_name.to_owned(),
        msg.session_id.clone(),
        String::new(),
        msg.project.clone(),
        tool_input,
        tool_response.to_owned(),
    );

    // Deterministic UUID v5 based on message CONTENT (not row ID) to prevent duplicates.
    // Using content-based hash avoids UUID collisions when the queue table is truncated
    // while observations persist — new messages reusing old row IDs won't collide.
    // If the same message is processed twice (race condition), same observation ID is generated.
    let id = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        tool_name.hash(&mut hasher);
        msg.session_id.hash(&mut hasher);
        tool_response.hash(&mut hasher);
        msg.created_at_epoch.hash(&mut hasher);
        let content_hash = hasher.finish();
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, &content_hash.to_le_bytes()).to_string()
    };

    let result = state.observation_service.process(&id, tool_call).await?;

    if let Some(observation) = result {
        tracing::info!("Processed pending message {} -> observation {}", msg.id, observation.id);
    } else {
        tracing::debug!("Observation filtered as trivial for message {}", msg.id);
    }

    Ok(())
}

/// Spawns background task that polls pending queue every 5 seconds.
pub fn start_background_processor(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        let mut loop_count: u64 = 0;
        loop {
            interval.tick().await;
            if !state.processing_active.load(Ordering::SeqCst) {
                continue;
            }

            // Periodic injection cleanup (~once per hour at 5s interval: 720 * 5s = 3600s)
            loop_count = loop_count.wrapping_add(1);
            if loop_count.is_multiple_of(720) {
                if let Err(e) = state.observation_service.cleanup_old_injections().await {
                    tracing::warn!(error = %e, "Periodic injection cleanup failed");
                }
            }

            tracing::debug!("Background processor: checking queue...");

            let max_workers = max_queue_workers();
            let messages = match state
                .queue_service
                .claim_pending_messages(max_workers, default_visibility_timeout_secs())
                .await
            {
                Ok(msgs) => msgs,
                Err(e) => {
                    tracing::error!("Background processor: claim failed: {}", e);
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
                            if let Err(e) = state_clone.queue_service.complete_message(msg.id).await
                            {
                                tracing::error!(
                                    "Background: complete message {} error: {}",
                                    msg.id,
                                    e
                                );
                            }
                        },
                        Err(e) => {
                            tracing::error!("Background: process message {} failed: {}", msg.id, e);
                            if let Err(e) =
                                state_clone.queue_service.fail_message(msg.id, true).await
                            {
                                tracing::error!("Background: fail message {} error: {}", msg.id, e);
                            }
                        },
                    }
                });
                handles.push(handle);
            }

            for handle in handles {
                if let Err(e) = handle.await {
                    tracing::error!("Background processor: task join error: {}", e);
                }
            }

            tracing::info!("Background processor: processed {} messages", count);
        }
    });
}

/// Releases stale messages back to pending queue on startup.
///
/// # Errors
/// Returns error if database operation fails.
pub async fn run_startup_recovery(state: &AppState) -> anyhow::Result<usize> {
    let released =
        state.queue_service.release_stale_messages(default_visibility_timeout_secs()).await?;
    if released > 0 {
        tracing::info!("Startup recovery: released {} stale messages back to pending", released);
    }

    let closed = state.session_service.close_stale_sessions(24).await?;
    if closed > 0 {
        tracing::info!("Startup recovery: closed {} stale sessions (>24h active)", closed);
    }

    match state.observation_service.cleanup_old_injections().await {
        Ok(cleaned) if cleaned > 0 => {
            tracing::info!("Startup recovery: cleaned {} stale injection records", cleaned);
        },
        Ok(_) => {},
        Err(e) => {
            tracing::warn!("Failed to clean up old injection records: {}", e);
        },
    }

    Ok(released)
}
