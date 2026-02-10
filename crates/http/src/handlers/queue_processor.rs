use std::env;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::spawn_blocking;

use opencode_mem_core::{is_low_value_observation, ObservationInput, ProjectFilter, ToolOutput};
use opencode_mem_infinite::tool_event;
use opencode_mem_storage::{default_visibility_timeout_secs, PendingMessage};

use crate::AppState;

pub(crate) fn max_queue_workers() -> usize {
    env::var("OPENCODE_MEM_QUEUE_WORKERS").ok().and_then(|v| v.parse().ok()).unwrap_or(10)
}

pub async fn process_pending_message(state: &AppState, msg: &PendingMessage) -> anyhow::Result<()> {
    if let Some(project) = msg.project.as_deref().filter(|p| !p.is_empty() && *p != "unknown") {
        if ProjectFilter::global().is_some_and(|filter| filter.is_excluded(project)) {
            tracing::debug!("Skipping excluded project '{}' for message {}", project, msg.id);
            return Ok(());
        }
    }

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
    let observation = state
        .llm
        .compress_to_observation(
            &id,
            &input,
            msg.project.as_deref().filter(|p| !p.is_empty() && *p != "unknown"),
        )
        .await?;

    // Store raw event in Infinite Memory REGARDLESS of whether observation is trivial.
    // Architecture invariant: raw events are NEVER lost — drill-down must always work.
    if let Some(infinite_mem) = state.infinite_mem.as_ref() {
        let files = observation.as_ref().map(|obs| obs.files_modified.clone()).unwrap_or_default();
        let event = tool_event(
            &msg.session_id,
            None,
            tool_name,
            tool_input
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(serde_json::Value::Null),
            serde_json::json!({"output": tool_response}),
            files,
        );
        infinite_mem.store_event(event).await?;
    }

    let Some(observation) = observation else {
        tracing::debug!("Observation filtered as trivial for message {}", msg.id);
        return Ok(());
    };

    if is_low_value_observation(&observation.title) {
        tracing::debug!("Filtered low-value observation from queue: {}", observation.title);
        return Ok(());
    }

    let storage = Arc::clone(&state.storage);
    let obs_clone = observation.clone();
    let inserted = spawn_blocking(move || storage.save_observation(&obs_clone))
        .await
        .map_err(|e| anyhow::anyhow!("save observation join error: {e}"))??;
    if !inserted {
        tracing::debug!("Skipping duplicate observation: {}", observation.title);
        return Ok(());
    }
    tracing::info!("Processed pending message {} -> observation {}", msg.id, observation.id);
    if let Err(e) = state.event_tx.send(serde_json::to_string(&observation)?) {
        tracing::debug!("No SSE subscribers for observation event: {}", e);
    }

    Ok(())
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
                            if let Err(e) = spawn_blocking(move || storage.complete_message(msg_id))
                                .await
                                .map_err(|e| anyhow::anyhow!("join error: {e}"))
                                .and_then(|r| r.map_err(|e| anyhow::anyhow!("{e}")))
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
                            let storage = Arc::clone(&state_clone.storage);
                            let msg_id = msg.id;
                            if let Err(e) =
                                spawn_blocking(move || storage.fail_message(msg_id, true))
                                    .await
                                    .map_err(|e| anyhow::anyhow!("join error: {e}"))
                                    .and_then(|r| r.map_err(|e| anyhow::anyhow!("{e}")))
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
pub fn run_startup_recovery(state: &AppState) -> anyhow::Result<usize> {
    let released = state.storage.release_stale_messages(default_visibility_timeout_secs())?;
    if released > 0 {
        tracing::info!("Startup recovery: released {} stale messages back to pending", released);
    }

    let closed = state.storage.close_stale_sessions(24)?;
    if closed > 0 {
        tracing::info!("Startup recovery: closed {} stale sessions (>24h active)", closed);
    }

    Ok(released)
}
