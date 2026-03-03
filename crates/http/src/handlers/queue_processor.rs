use crate::api_error::ApiError;
use std::sync::atomic::Ordering;
use std::sync::Arc;

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
        .and_then(|s| {
            serde_json::from_str(s)
                .map_err(|e| {
                    tracing::warn!(
                        error = %e,
                        "Failed to parse tool_input for pending message {}",
                        msg.id
                    );
                })
                .ok()
        })
        .unwrap_or(serde_json::Value::Null);
    let tool_response = msg.tool_response.as_deref().unwrap_or("");

    let id = {
        let input_str = msg.tool_input.as_deref().unwrap_or("");
        let mut data = String::with_capacity(
            tool_name.len() + msg.session_id.len() + input_str.len() + tool_response.len() + 20,
        );
        data.push_str(tool_name);
        data.push_str(&msg.session_id);
        data.push_str(input_str);
        data.push_str(tool_response);
        data.push_str(&msg.created_at_epoch.to_string());
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, data.as_bytes()).to_string()
    };

    let tool_call = ToolCall::new(
        tool_name.to_owned(),
        msg.session_id.clone(),
        id.clone(),
        msg.project.clone(),
        tool_input,
        tool_response.to_owned(),
    );

    let result = state.observation_service.process(&id, tool_call).await?;

    if let Some(observation) = result {
        tracing::info!("Processed pending message {} -> observation {}", msg.id, observation.id);
    } else {
        tracing::debug!("Observation filtered as trivial for message {}", msg.id);
    }

    Ok(())
}

/// Latency-sensitive queue poller. Checks for pending messages every 5 seconds
/// and spawns fire-and-forget tasks for each message.
///
/// Runs until the process receives a shutdown signal (ctrl+c).
pub async fn start_queue_poller(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    loop {
        tokio::select! {
            _ = interval.tick() => {},
            _ = shutdown_rx.recv() => {
                tracing::info!("Queue poller: shutting down");
                return;
            }
        }

        if !state.processing_active.load(Ordering::SeqCst) {
            continue;
        }

        tracing::debug!("Background processor: checking queue...");

        let max_workers = max_queue_workers();
        let available_permits = state.semaphore.available_permits().min(max_workers);

        if available_permits == 0 {
            continue;
        }

        let messages = match state
            .queue_service
            .claim_pending_messages(available_permits, default_visibility_timeout_secs())
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

        let mut spawned = 0;
        let mut unspawned = Vec::new();
        
        for msg in messages {
            let permit = match Arc::clone(&state.semaphore).try_acquire_owned() {
                Ok(p) => p,
                Err(_) => {
                    unspawned.push(msg);
                    continue;
                }
            };
            
            spawned += 1;
            let state_clone = Arc::clone(&state);
            tokio::spawn(async move {
                let _permit = permit;
                let result = process_pending_message(&state_clone, &msg).await;
                match result {
                    Ok(()) => {
                        if let Err(e) = state_clone.queue_service.complete_message(msg.id).await {
                            tracing::error!(
                                "Background: complete message {} error: {}",
                                msg.id,
                                e
                            );
                        }
                    },
                    Err(e) => {
                        tracing::error!("Background: process message {} failed: {}", msg.id, e);
                        if let Err(e) = state_clone.queue_service.fail_message(msg.id, true).await
                        {
                            tracing::error!("Background: fail message {} error: {}", msg.id, e);
                        }
                    },
                }
            });
        }
        
        if !unspawned.is_empty() {
            let unspawned_ids: Vec<i64> = unspawned.iter().map(|m| m.id).collect();
            if let Err(e) = state.queue_service.release_messages(&unspawned_ids).await {
                tracing::error!("Background processor: failed to release unspawned messages: {}", e);
            }
        }
        
        if spawned > 0 {
            tracing::info!("Background processor: spawned {} message tasks", spawned);
        }

        
    }
}

/// Periodic maintenance scheduler. Runs batch operations on longer intervals:
/// - Infinite memory compression (~5 min)
/// - Embedding backfill (~15 min)
/// - Dedup sweep (~30 min)
/// - Injection cleanup (~1 hour)
/// - DLQ garbage collection (~1 day)
///
/// Runs until the process receives a shutdown signal (ctrl+c).
pub async fn start_cron_scheduler(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    let mut loop_count: u64 = 0;
    loop {
        tokio::select! {
            _ = interval.tick() => {},
            _ = shutdown_rx.recv() => {
                tracing::info!("Cron scheduler: shutting down");
                return;
            }
        }

        if !state.processing_active.load(Ordering::SeqCst) {
            continue;
        }

        loop_count = loop_count.wrapping_add(1);

        if loop_count.is_multiple_of(60) {
            if let Some(ref infinite_mem) = state.infinite_mem {
                tracing::debug!("Cron: running infinite memory compression...");
                let mem = Arc::clone(infinite_mem);
                tokio::spawn(async move {
                    match mem.run_full_compression().await {
                        Ok((five_min, hour, day)) => {
                            if five_min > 0 || hour > 0 || day > 0 {
                                tracing::info!(
                                    "Cron: created {} 5min, {} hour, {} day summaries",
                                    five_min, hour, day,
                                );
                            }
                        },
                        Err(e) => tracing::warn!("Cron: infinite memory error: {e:?}"),
                    }
                });
            }
        }

        if loop_count.is_multiple_of(180) {
            tracing::debug!("Cron: running embedding backfill...");
            let state_clone = Arc::clone(&state);
            tokio::spawn(async move {
                match state_clone.search_service.run_embedding_backfill(100).await {
                    Ok(generated) if generated > 0 => {
                        tracing::info!("Cron: generated {} embeddings", generated);
                    },
                    Ok(_) => {},
                    Err(e) => tracing::warn!("Cron: embedding backfill failed: {}", e),
                }
            });
        }

        if loop_count.is_multiple_of(360) {
            let state_clone = Arc::clone(&state);
            tokio::spawn(async move {
                match state_clone.observation_service.run_dedup_sweep().await {
                    Ok(merged) if merged > 0 => {
                        tracing::info!(merged, "Cron: dedup sweep completed");
                    },
                    Ok(_) => {},
                    Err(e) => tracing::warn!(error = %e, "Cron: dedup sweep failed"),
                }
            });
        }

        if loop_count.is_multiple_of(720) {
            let state_clone = Arc::clone(&state);
            tokio::spawn(async move {
                if let Err(e) = state_clone.observation_service.cleanup_old_injections().await {
                    tracing::warn!(error = %e, "Cron: injection cleanup failed");
                }
            });
        }

        if loop_count.is_multiple_of(17280) {
            let ttl_secs =
                opencode_mem_core::env_parse_with_default("OPENCODE_MEM_DLQ_TTL_DAYS", 7_i64)
                    * 86400;
            let state_clone = Arc::clone(&state);
            tokio::spawn(async move {
                match state_clone.queue_service.clear_stale_failed_messages(ttl_secs).await {
                    Ok(deleted) if deleted > 0 => {
                        tracing::info!(deleted, "Cron: DLQ garbage collection completed");
                    },
                    Ok(_) => {},
                    Err(e) => tracing::warn!(error = %e, "Cron: DLQ garbage collection failed"),
                }
            });
        }
    }
}

/// Spawns independent background tasks for queue polling and periodic maintenance.
///
/// Two independent `tokio::spawn` tasks ensure that long-running cron operations
/// (dedup sweep, embedding backfill, infinite memory compression) never block
/// latency-sensitive queue processing.
pub fn start_background_processor(state: Arc<AppState>) {
    let state_poller = Arc::clone(&state);
    tokio::spawn(async move {
        start_queue_poller(state_poller).await;
    });

    let state_cron = Arc::clone(&state);
    tokio::spawn(async move {
        start_cron_scheduler(state_cron).await;
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
