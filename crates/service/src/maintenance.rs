use std::sync::Arc;

use crate::{
    InfiniteMemoryService, KnowledgeService, ObservationService, QueueService, SearchService,
    SessionService,
};
use opencode_mem_core::AppConfig;

pub struct MaintenanceServices {
    pub observation_service: Arc<ObservationService>,
    pub session_service: Arc<SessionService>,
    pub knowledge_service: Arc<KnowledgeService>,
    pub search_service: Arc<SearchService>,
    pub queue_service: Arc<QueueService>,
    pub infinite_mem: Option<Arc<InfiniteMemoryService>>,
    pub config: Arc<AppConfig>,
}

pub async fn run_maintenance_tick(services: &MaintenanceServices, loop_count: u64) {
    if loop_count.is_multiple_of(60)
        && let Some(ref mem) = services.infinite_mem
    {
        tracing::debug!("Maintenance: running infinite memory compression...");
        let mem = Arc::clone(mem);
        tokio::spawn(async move {
            match mem.run_full_compression().await {
                Ok((five_min, hour, day)) => {
                    if five_min > 0 || hour > 0 || day > 0 {
                        tracing::info!(
                            "Maintenance: created {} 5min, {} hour, {} day summaries",
                            five_min,
                            hour,
                            day,
                        );
                    }
                }
                Err(e) => tracing::warn!("Maintenance: infinite memory error: {e:?}"),
            }
        });
    }

    if loop_count.is_multiple_of(180) {
        tracing::debug!("Maintenance: running embedding backfill...");
        let svc = Arc::clone(&services.search_service);
        tokio::spawn(async move {
            match svc.run_embedding_backfill(100).await {
                Ok(generated) if generated > 0 => {
                    tracing::info!("Maintenance: generated {} embeddings", generated);
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("Maintenance: embedding backfill failed: {}", e),
            }
        });
    }

    if loop_count.is_multiple_of(360) {
        let svc = Arc::clone(&services.observation_service);
        tokio::spawn(async move {
            match svc.run_dedup_sweep().await {
                Ok(merged) if merged > 0 => {
                    tracing::info!(merged, "Maintenance: dedup sweep completed");
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "Maintenance: dedup sweep failed"),
            }
        });
    }

    if loop_count.is_multiple_of(720) {
        let svc = Arc::clone(&services.observation_service);
        tokio::spawn(async move {
            if let Err(e) = svc.cleanup_old_injections().await {
                tracing::warn!(error = %e, "Maintenance: injection cleanup failed");
            }
        });
    }

    if loop_count.is_multiple_of(17280) {
        let ttl_secs = services.config.dlq_ttl_secs();
        let svc = Arc::clone(&services.queue_service);
        tokio::spawn(async move {
            match svc.clear_stale_failed_messages(ttl_secs).await {
                Ok(deleted) if deleted > 0 => {
                    tracing::info!(deleted, "Maintenance: DLQ garbage collection completed");
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "Maintenance: DLQ GC failed"),
            }
        });
    }

    if loop_count.is_multiple_of(2160) {
        let svc = Arc::clone(&services.knowledge_service);
        tokio::spawn(async move {
            match svc.run_confidence_lifecycle().await {
                Ok((decayed, archived)) if decayed > 0 || archived > 0 => {
                    tracing::info!(
                        decayed,
                        archived,
                        "Maintenance: knowledge confidence lifecycle completed"
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "Maintenance: knowledge lifecycle failed");
                }
            }
        });
    }

    if loop_count.is_multiple_of(360) {
        let svc = Arc::clone(&services.session_service);
        tokio::spawn(async move {
            match svc.generate_pending_summaries(10).await {
                Ok(n) if n > 0 => {
                    tracing::info!(
                        generated = n,
                        "Maintenance: session summary generation completed"
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "Maintenance: session summary generation failed");
                }
            }
        });
    }
}
