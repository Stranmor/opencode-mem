use super::*;
use opencode_mem_core::{Observation, ObservationType};
use opencode_mem_service::{PendingWriteQueue, SearchService};
use opencode_mem_storage::{StorageBackend, traits::ObservationStore};
use serde_json::json;
use std::sync::Arc;

async fn setup_storage() -> StorageBackend {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
    StorageBackend::new(&url)
        .await
        .expect("Failed to connect to PG")
}

fn setup_search_service(backend: StorageBackend) -> SearchService {
    SearchService::new(Arc::new(backend), None, None)
}

fn setup_observation_service(backend: StorageBackend) -> opencode_mem_service::ObservationService {
    let (event_tx, _rx) = tokio::sync::broadcast::channel(16);
    let config = opencode_mem_core::AppConfig {
        database_url: String::new(),
        api_key: String::new(),
        api_url: String::new(),
        model: String::new(),
        disable_embeddings: true,
        embedding_threads: 0,
        infinite_memory_url: None,
        dedup_threshold: 0.85,
        injection_dedup_threshold: 0.80,
        queue_workers: 10,
        max_retry: 3,
        visibility_timeout_secs: 300,
        dlq_ttl_days: 7,
        max_content_chars: 500,
        max_total_chars: 8000,
        max_events: 200,
    };
    opencode_mem_service::ObservationService::new(
        Arc::new(backend),
        Arc::new(
            opencode_mem_llm::LlmClient::new(String::new(), String::new(), String::new()).unwrap(),
        ),
        None,
        event_tx,
        None,
        &config,
    )
}

#[path = "memory_tests_save.rs"]
mod save;

#[path = "memory_tests_search.rs"]
mod search;
