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
use sqlx::postgres::PgPoolOptions;
use std::sync::{Arc, Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn setup_degraded_services() -> (
    Arc<ObservationService>,
    Arc<SessionService>,
    Arc<KnowledgeService>,
    Arc<SearchService>,
    Arc<PendingWriteQueue>,
) {
    let _guard = env_lock().lock().expect("lock");
    // SAFETY: Test-only, serialized by env_lock mutex
    unsafe {
        std::env::set_var("DATABASE_URL", "postgres://bogus:bogus@127.0.0.1:1/bogus");
        std::env::set_var(
            "INFINITE_MEMORY_URL",
            "postgres://bogus:bogus@127.0.0.1:1/bogus",
        );
        std::env::set_var("OPENCODE_MEM_API_KEY", "test-key");
    }

    let config = opencode_mem_core::AppConfig::from_env().expect("valid config");

    let backend = Arc::new(StorageBackend::new_degraded(
        "postgres://bogus:bogus@127.0.0.1:1/bogus",
    ));

    let (event_tx, _rx) = tokio::sync::broadcast::channel(16);
    let llm = Arc::new(
        opencode_mem_llm::LlmClient::new(String::new(), String::new(), String::new()).unwrap(),
    );

    let infinite_mem = if let Ok(p) = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_millis(1))
        .connect_lazy("postgres://bogus:bogus@127.0.0.1:1/bogus")
    {
        Some(Arc::new(
            opencode_mem_service::InfiniteMemoryService::new_degraded(p, llm.clone()),
        ))
    } else {
        None
    };

    let observation_service = Arc::new(ObservationService::new(
        backend.clone(),
        llm.clone(),
        infinite_mem.clone(),
        event_tx,
        None,
        &config,
    ));
    let session_service = Arc::new(SessionService::new(backend.clone(), llm.clone()));
    let knowledge_service = Arc::new(KnowledgeService::new(backend.clone(), None));
    let search_service = Arc::new(SearchService::new(
        backend,
        None,
        infinite_mem.clone(),
        0.85,
    ));
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
        "infinite_drill_day" => json!({"id": 1}),
        "infinite_drill_hour" => json!({"id": 1}),
        "infinite_search_entities" => json!({"type": "file", "value": "test.rs"}),
        _ => json!({}),
    }
}

const INFINITE_TOOLS: [&str; 5] = [
    "infinite_expand",
    "infinite_time_range",
    "infinite_drill_day",
    "infinite_drill_hour",
    "infinite_search_entities",
];
