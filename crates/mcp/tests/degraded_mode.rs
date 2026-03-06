#![allow(clippy::unwrap_used, reason = "test code")]
#![allow(clippy::indexing_slicing, reason = "test code with known structure")]
#![allow(clippy::missing_docs_in_private_items, reason = "test code")]
#![allow(missing_docs, reason = "test code")]
#![allow(clippy::implicit_return, reason = "test code")]
#![allow(clippy::question_mark_used, reason = "test code")]
#![allow(
    clippy::panic,
    reason = "test assertions use panic for descriptive failure messages"
)]
#![allow(
    clippy::needless_borrow,
    reason = "borrow needed for contains() on &str slice"
)]

#[path = "degraded_mode/read_tools_tests.rs"]
mod read_tools_tests;
#[path = "degraded_mode/write_tools_tests.rs"]
mod write_tools_tests;

use opencode_mem_mcp::McpTool;
use opencode_mem_service::{
    KnowledgeService, ObservationService, PendingWriteQueue, SearchService, SessionService,
};
use opencode_mem_storage::StorageBackend;
use serde_json::json;
use std::sync::Arc;

fn setup_degraded_services() -> (
    Arc<ObservationService>,
    Arc<SessionService>,
    Arc<KnowledgeService>,
    Arc<SearchService>,
    Arc<PendingWriteQueue>,
) {
    let backend = Arc::new(StorageBackend::new_degraded(
        "postgres://bogus:bogus@127.0.0.1:1/bogus",
    ));

    let (event_tx, _rx) = tokio::sync::broadcast::channel(16);
    let llm = Arc::new(
        opencode_mem_llm::LlmClient::new(String::new(), String::new(), String::new()).unwrap(),
    );

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

    let observation_service = Arc::new(ObservationService::new(
        backend.clone(),
        llm.clone(),
        None,
        event_tx,
        None,
        &config,
    ));
    let session_service = Arc::new(SessionService::new(backend.clone(), llm));
    let knowledge_service = Arc::new(KnowledgeService::new(backend.clone()));
    let search_service = Arc::new(SearchService::new(backend, None, None));
    let pending_writes = Arc::new(PendingWriteQueue::new());

    (
        observation_service,
        session_service,
        knowledge_service,
        search_service,
        pending_writes,
    )
}

fn tool_args(tool_name: &str) -> serde_json::Value {
    match tool_name {
        "__IMPORTANT" => json!({}),
        "search" => json!({"query": "test"}),
        "timeline" => json!({}),
        "get_observations" => json!({"ids": ["test-id-1"]}),
        "memory_get" => json!({"id": "nonexistent-id"}),
        "memory_recent" => json!({}),
        "memory_hybrid_search" => json!({"query": "test"}),
        "memory_semantic_search" => json!({"query": "test"}),
        "save_memory" => json!({"text": "test memory content"}),
        "knowledge_search" => json!({"query": "test"}),
        "knowledge_save" => json!({
            "knowledge_type": "skill",
            "title": "Test Skill",
            "description": "A test skill description"
        }),
        "knowledge_get" => json!({"id": "nonexistent-id"}),
        "knowledge_list" => json!({}),
        "knowledge_delete" => json!({"id": "nonexistent-id"}),
        "memory_delete" => json!({"id": "nonexistent-id"}),
        "infinite_expand" => json!({"id": 1}),
        "infinite_time_range" => json!({
            "start": "2025-01-01T00:00:00Z",
            "end": "2025-01-02T00:00:00Z"
        }),
        "infinite_drill_hour" => json!({"id": 1}),
        "infinite_drill_minute" => json!({"id": 1}),
        _ => json!({}),
    }
}

const INFINITE_TOOLS: [&str; 4] = [
    "infinite_expand",
    "infinite_time_range",
    "infinite_drill_hour",
    "infinite_drill_minute",
];
