use std::sync::Arc;
use std::sync::atomic::Ordering;

use opencode_mem_core::ToolCall;
use opencode_mem_service::{PendingMessage, QueueService};

use crate::AppState;

pub(crate) fn max_queue_workers(state: &AppState) -> usize {
    state.config.queue_workers
}

pub async fn process_pending_message(state: &AppState, msg: &PendingMessage) -> anyhow::Result<()> {
    if let Some(project) = msg.project.as_deref() {
        if QueueService::should_skip_project(Some(project)) {
            tracing::debug!("Skipping project '{}' for message {}", project, msg.id);
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

    // Use msg.call_id if present, otherwise generate a deterministic UUID
    let id = if let Some(ref cid) = msg.call_id {
        cid.clone()
    } else {
        let input_str = msg.tool_input.as_deref().unwrap_or("");
        let mut data = String::with_capacity(
            tool_name.len() + msg.session_id.len() + input_str.len() + tool_response.len() + 24,
        );
        data.push_str(tool_name);
        data.push('\0');
        data.push_str(&msg.session_id);
        data.push('\0');
        data.push_str(input_str);
        data.push('\0');
        data.push_str(tool_response);
        data.push('\0');
        data.push_str(&msg.created_at_epoch.to_string());
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, data.as_bytes()).to_string()
    };

    let tool_call = ToolCall::new(
        tool_name.to_owned(),
        opencode_mem_core::SessionId(msg.session_id.clone()),
        id.clone(),
        msg.project.clone(),
        tool_input,
        tool_response.to_owned(),
    );

    let result = state.observation_service.process(&id, tool_call).await?;

    if let Some(observation) = result {
        tracing::info!(
            "Processed pending message {} -> observation {}",
            msg.id,
            observation.id
        );
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
    let poll_interval = std::time::Duration::from_secs(5);
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    loop {
        if !state.processing_active.load(Ordering::SeqCst) {
            tokio::select! {
                _ = tokio::time::sleep(poll_interval) => {},
                _ = shutdown_rx.recv() => {
                    tracing::info!("Queue poller: shutting down");
                    return;
                }
            }
            continue;
        }

        tracing::debug!("Background processor: checking queue...");

        let max_workers = max_queue_workers(&state);
        let available_permits = state.semaphore.available_permits().min(max_workers);

        if available_permits == 0 {
            tokio::select! {
                _ = tokio::time::sleep(poll_interval) => {},
                _ = shutdown_rx.recv() => {
                    tracing::info!("Queue poller: shutting down");
                    return;
                }
            }
            continue;
        }

        let messages = match state
            .queue_service
            .claim_pending_messages(available_permits, state.config.visibility_timeout_secs)
            .await
        {
            Ok(msgs) => msgs,
            Err(e) => {
                tracing::error!("Background processor: claim failed: {}", e);
                tokio::select! {
                    _ = tokio::time::sleep(poll_interval) => {},
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Queue poller: shutting down");
                        return;
                    }
                }
                continue;
            }
        };

        let got_work = !messages.is_empty();

        if !messages.is_empty() {
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
                            if let Err(e) = state_clone.queue_service.complete_message(msg.id).await
                            {
                                tracing::error!(
                                    "Background: complete message {} error: {}",
                                    msg.id,
                                    e
                                );
                            }
                        }
                        Err(e) => {
                            tracing::error!("Background: process message {} failed: {}", msg.id, e);
                            if let Err(e) =
                                state_clone.queue_service.fail_message(msg.id, true).await
                            {
                                tracing::error!("Background: fail message {} error: {}", msg.id, e);
                            }
                        }
                    }
                });
            }

            if !unspawned.is_empty() {
                let unspawned_ids: Vec<i64> = unspawned.iter().map(|m| m.id).collect();
                if let Err(e) = state.queue_service.release_messages(&unspawned_ids).await {
                    tracing::error!(
                        "Background processor: failed to release unspawned messages: {}",
                        e
                    );
                }
            }

            if spawned > 0 {
                tracing::info!("Background processor: spawned {} message tasks", spawned);
            }
        }

        // If we processed work, loop immediately to check for more.
        // If idle, sleep before next poll.
        if !got_work {
            tokio::select! {
                _ = tokio::time::sleep(poll_interval) => {},
                _ = shutdown_rx.recv() => {
                    tracing::info!("Queue poller: shutting down");
                    return;
                }
            }
        }
    }
}

pub fn start_background_processor(state: Arc<AppState>) {
    let state_poller = Arc::clone(&state);
    tokio::spawn(async move {
        start_queue_poller(state_poller).await;
    });

    let state_cron = Arc::clone(&state);
    tokio::spawn(async move {
        super::cron::start_cron_scheduler(state_cron).await;
    });
}

/// Releases stale messages back to pending queue on startup.
///
/// # Errors
/// Returns error if database operation fails.
pub async fn run_startup_recovery(state: &AppState) -> anyhow::Result<usize> {
    let released = state
        .queue_service
        .release_stale_messages(state.config.visibility_timeout_secs)
        .await?;
    if released > 0 {
        tracing::info!(
            "Startup recovery: released {} stale messages back to pending",
            released
        );
    }

    let closed = state.session_service.close_stale_sessions(24).await?;
    if closed > 0 {
        tracing::info!(
            "Startup recovery: closed {} stale sessions (>24h active)",
            closed
        );
    }

    match state.observation_service.cleanup_old_injections().await {
        Ok(cleaned) if cleaned > 0 => {
            tracing::info!(
                "Startup recovery: cleaned {} stale injection records",
                cleaned
            );
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!("Failed to clean up old injection records: {}", e);
        }
    }

    Ok(released)
}
