use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::AppState;

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

        if loop_count.is_multiple_of(60)
            && let Some(ref infinite_mem) = state.infinite_mem
        {
            tracing::debug!("Cron: running infinite memory compression...");
            let mem = Arc::clone(infinite_mem);
            tokio::spawn(async move {
                match mem.run_full_compression().await {
                    Ok((five_min, hour, day)) => {
                        if five_min > 0 || hour > 0 || day > 0 {
                            tracing::info!(
                                "Cron: created {} 5min, {} hour, {} day summaries",
                                five_min,
                                hour,
                                day,
                            );
                        }
                    }
                    Err(e) => tracing::warn!("Cron: infinite memory error: {e:?}"),
                }
            });
        }

        if loop_count.is_multiple_of(180) {
            tracing::debug!("Cron: running embedding backfill...");
            let state_clone = Arc::clone(&state);
            tokio::spawn(async move {
                match state_clone.search_service.run_embedding_backfill(100).await {
                    Ok(generated) if generated > 0 => {
                        tracing::info!("Cron: generated {} embeddings", generated);
                    }
                    Ok(_) => {}
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
                    }
                    Ok(_) => {}
                    Err(e) => tracing::warn!(error = %e, "Cron: dedup sweep failed"),
                }
            });
        }

        if loop_count.is_multiple_of(720) {
            let state_clone = Arc::clone(&state);
            tokio::spawn(async move {
                if let Err(e) = state_clone
                    .observation_service
                    .cleanup_old_injections()
                    .await
                {
                    tracing::warn!(error = %e, "Cron: injection cleanup failed");
                }
            });
        }

        if loop_count.is_multiple_of(17280) {
            let ttl_secs = state.config.dlq_ttl_secs();
            let state_clone = Arc::clone(&state);
            tokio::spawn(async move {
                match state_clone
                    .queue_service
                    .clear_stale_failed_messages(ttl_secs)
                    .await
                {
                    Ok(deleted) if deleted > 0 => {
                        tracing::info!(deleted, "Cron: DLQ garbage collection completed");
                    }
                    Ok(_) => {}
                    Err(e) => tracing::warn!(error = %e, "Cron: DLQ garbage collection failed"),
                }
            });
        }

        if loop_count.is_multiple_of(2160) {
            let state_clone = Arc::clone(&state);
            tokio::spawn(async move {
                match state_clone
                    .knowledge_service
                    .run_confidence_lifecycle()
                    .await
                {
                    Ok((decayed, archived)) if decayed > 0 || archived > 0 => {
                        tracing::info!(
                            decayed,
                            archived,
                            "Cron: knowledge confidence lifecycle completed"
                        );
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "Cron: knowledge confidence lifecycle failed");
                    }
                }
            });
        }
    }
}
