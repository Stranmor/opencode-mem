use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::AppState;
use opencode_mem_service::maintenance::{MaintenanceServices, run_maintenance_tick};

pub async fn start_cron_scheduler(state: Arc<AppState>) {
    let services = MaintenanceServices {
        observation_service: Arc::clone(&state.observation_service),
        session_service: Arc::clone(&state.session_service),
        knowledge_service: Arc::clone(&state.knowledge_service),
        search_service: Arc::clone(&state.search_service),
        queue_service: Arc::clone(&state.queue_service),
        infinite_mem: state.infinite_mem.clone(),
        config: Arc::clone(&state.config),
    };

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

        run_maintenance_tick(&services, loop_count).await;
    }
}
