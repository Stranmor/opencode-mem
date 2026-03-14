use std::sync::Arc;
use std::sync::atomic::Ordering;

use opencode_mem_core::ToolCall;
use opencode_mem_service::{PendingMessage, QueueService};

use crate::AppState;

pub(crate) fn max_queue_workers(state: &AppState) -> usize {
    state.config.queue_workers
}

pub async fn process_pending_message(state: &AppState, msg: &PendingMessage) -> anyhow::Result<()> {
    if state
        .queue_service
        .should_skip_project(msg.project.as_deref())
    {
        tracing::debug!(
            "Skipping project '{:?}' for message {}",
            msg.project,
            msg.id
        );
        return Ok(());
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
            tool_name.len() + msg.session_id.len() + input_str.len() + tool_response.len() + 48,
        );
        data.push_str(tool_name);
        data.push('\0');
        data.push_str(&msg.session_id);
        data.push('\0');
        data.push_str(input_str);
        data.push('\0');
        data.push_str(tool_response);
        data.push('\0');
        // Include msg.id to guarantee uniqueness across identical tool calls in the same second
        data.push_str(&msg.id.to_string());
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

        // Reserve permits synchronously before DB query to avoid thundering herd.
        let mut permits = Vec::with_capacity(available_permits);
        for _ in 0..available_permits {
            if let Ok(p) = Arc::clone(&state.semaphore).try_acquire_owned() {
                permits.push(p);
            } else {
                break;
            }
        }

        if permits.is_empty() {
            tokio::select! {
                _ = tokio::time::sleep(poll_interval) => {},
                _ = shutdown_rx.recv() => {
                    tracing::info!("Queue poller: shutting down");
                    return;
                }
            }
            continue;
        }

        let claim_limit = permits.len();
        let messages = match state
            .queue_service
            .claim_pending_messages(claim_limit, state.config.visibility_timeout_secs)
            .await
        {
            Ok(msgs) => msgs,
            Err(e) => {
                tracing::error!("Background processor: claim failed: {}", e);
                // Permits will be automatically released when 'permits' vector is dropped.
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
            // Any unused permits will be dropped at the end of this loop iteration.

            // Before processing new messages, check if we need to flush pending writes from degraded mode.
            // This ensures HTTP-only deployments also recover stage data.
            if state.search_service.circuit_breaker().is_closed() {
                opencode_mem_service::spawn_pending_flush(
                    &state.observation_service,
                    &state.knowledge_service,
                    &state.pending_writes,
                );
            }

            for msg in messages {
                let permit = permits
                    .pop()
                    .expect("permit must exist for claimed message");
                spawned += 1;
                let state_clone = Arc::clone(&state);
                state.background_tasks.lock().await.spawn(async move {
                    let _permit = permit;
                    use futures_util::FutureExt;
                    let result =
                        std::panic::AssertUnwindSafe(process_pending_message(&state_clone, &msg))
                            .catch_unwind()
                            .await
                            .unwrap_or_else(|_| {
                                Err(anyhow::anyhow!(
                                    "Panic during background message processing"
                                ))
                            });

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
                                state_clone.queue_service.fail_message(msg.id, false).await
                            {
                                tracing::error!("Background: fail message {} error: {}", msg.id, e);
                            }
                        }
                    }
                });
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

    let state_tasks = Arc::clone(&state);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let mut tasks = state_tasks.background_tasks.lock().await;
            // Drain completed tasks from JoinSet to prevent memory leak
            while let Some(res) = tasks.try_join_next() {
                if let Err(e) = res
                    && e.is_panic()
                {
                    tracing::error!("Background task panicked: {}", e);
                }
            }
        }
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
