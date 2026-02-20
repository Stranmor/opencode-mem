//! HTTP API server for opencode-mem.

#![allow(missing_docs, reason = "Internal crate with self-explanatory API")]
#![allow(unreachable_pub, reason = "pub items are re-exported")]
#![allow(clippy::absolute_paths, reason = "Explicit paths for clarity")]
#![allow(unused_results, reason = "Some results are intentionally ignored")]
#![allow(clippy::arithmetic_side_effects, reason = "Arithmetic is safe in context")]
#![allow(clippy::exit, reason = "Exit is used for graceful shutdown")]
#![allow(missing_copy_implementations, reason = "Types may grow")]
#![allow(clippy::let_underscore_untyped, reason = "Type is clear from context")]
#![allow(let_underscore_drop, reason = "Intentionally dropping values")]
#![allow(clippy::ref_patterns, reason = "Ref patterns are clearer")]
#![allow(missing_debug_implementations, reason = "Internal types")]
#![allow(unused_imports, reason = "Imports may be used conditionally")]
#![allow(dead_code, reason = "Public API for future use")]
#![allow(clippy::needless_continue, reason = "Continue is clearer in loops")]
#![allow(clippy::missing_docs_in_private_items, reason = "Internal crate")]
#![allow(clippy::implicit_return, reason = "Implicit return is idiomatic Rust")]
#![allow(clippy::question_mark_used, reason = "? operator is idiomatic Rust")]
#![allow(clippy::min_ident_chars, reason = "Short closure params are idiomatic")]
#![allow(clippy::else_if_without_else, reason = "Else not always needed")]
#![allow(clippy::shadow_reuse, reason = "Shadowing for Arc clones is idiomatic")]
#![allow(clippy::shadow_unrelated, reason = "Shadowing in async blocks is idiomatic")]
#![allow(clippy::exhaustive_structs, reason = "HTTP types are stable")]
#![allow(clippy::single_call_fn, reason = "Helper functions improve readability")]

pub mod api_error;
mod api_types;
mod blocking;
mod handlers;
mod query_types;
mod response_types;
mod viewer;

use axum::{
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock, Semaphore};

use opencode_mem_infinite::InfiniteMemory;
use opencode_mem_service::{
    KnowledgeService, ObservationService, QueueService, SearchService, SessionService,
};

pub use api_types::{ReadinessResponse, Settings, VersionResponse};
pub use handlers::queue_processor::{run_startup_recovery, start_background_processor};

/// Spawns background task that runs infinite memory compression every 5 minutes.
///
/// Generates hierarchical summaries (5min → hour → day) from raw events.
/// Errors are logged but do not stop the loop — retries on next interval.
pub fn start_compression_pipeline(infinite_mem: Arc<InfiniteMemory>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            tracing::debug!("Compression pipeline: running full compression...");
            let mem = Arc::clone(&infinite_mem);
            let result = tokio::spawn(async move { mem.run_full_compression().await }).await;
            match result {
                Ok(Ok((five_min, hour, day))) => {
                    if five_min > 0 || hour > 0 || day > 0 {
                        tracing::info!(
                            "Compression pipeline: created {} 5min, {} hour, {} day summaries",
                            five_min,
                            hour,
                            day,
                        );
                    }
                },
                Ok(Err(e)) => {
                    tracing::warn!("Compression pipeline error: {e:?}");
                },
                Err(e) => {
                    tracing::warn!("Compression pipeline panic: {e:?}");
                },
            }
        }
    });
}

/// Shared application state for all HTTP handlers.
///
/// Contains service instances and infrastructure (SSE, semaphore, settings).
/// Wrapped in `Arc` for thread-safe sharing across handlers.
pub struct AppState {
    /// Semaphore limiting concurrent queue processing
    pub semaphore: Arc<Semaphore>,
    /// Broadcast channel for SSE real-time updates
    pub event_tx: broadcast::Sender<String>,
    /// Flag indicating if queue processing is active
    pub processing_active: AtomicBool,
    /// Runtime-configurable settings
    pub settings: RwLock<Settings>,
    /// Optional `PostgreSQL` backend for infinite memory
    pub infinite_mem: Option<Arc<InfiniteMemory>>,
    /// Service for processing observations
    pub observation_service: Arc<ObservationService>,
    /// Service for session management
    pub session_service: Arc<SessionService>,
    /// Service for knowledge operations
    pub knowledge_service: Arc<KnowledgeService>,
    /// Service for search and read-only query operations
    pub search_service: Arc<SearchService>,
    /// Service for pending message queue operations
    pub queue_service: Arc<QueueService>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(viewer::serve_viewer))
        .route("/health", get(health))
        .route("/api/readiness", get(readiness))
        .route("/api/version", get(version))
        .route("/observe", post(handlers::observations::observe))
        .route("/api/observe/batch", post(handlers::observations::observe_batch))
        .route("/api/memory/save", post(handlers::observations::save_memory))
        .route("/search", get(handlers::search::search))
        .route("/hybrid-search", get(handlers::search::hybrid_search))
        .route("/semantic-search", get(handlers::search::semantic_search))
        .route("/observations/{id}", get(handlers::observations::get_observation))
        .route("/observations/batch", post(handlers::observations::get_observations_batch))
        .route("/api/observations", get(handlers::observations::get_observations_paginated))
        .route("/api/summaries", get(handlers::observations::get_summaries_paginated))
        .route("/api/prompts", get(handlers::observations::get_prompts_paginated))
        .route("/api/session/{id}", get(handlers::observations::get_session_by_id))
        .route("/api/prompt/{id}", get(handlers::observations::get_prompt_by_id))
        .route("/api/search/observations", get(handlers::search::search))
        .route("/api/search/by-type", get(handlers::search::search))
        .route("/api/search/by-concept", get(handlers::search::search))
        .route("/api/search/sessions", get(handlers::search::search_sessions))
        .route("/api/search/prompts", get(handlers::search::search_prompts))
        .route("/api/search/by-file", get(handlers::search::search_by_file))
        .route("/api/context/recent", get(handlers::context::get_context_recent))
        .route("/recent", get(handlers::observations::get_recent))
        .route("/timeline", get(handlers::observations::get_timeline))
        .route("/projects", get(handlers::context::get_projects))
        .route("/stats", get(handlers::context::get_stats))
        .route("/context/inject", get(handlers::context::get_context_recent))
        .route("/session/summary", post(handlers::sessions::generate_summary))
        .route("/events", get(handlers::context::sse_events))
        .route("/sessions/{sessionDbId}/init", post(handlers::sessions::session_init_legacy))
        .route(
            "/sessions/{sessionDbId}/observations",
            post(handlers::sessions::session_observations_legacy),
        )
        .route(
            "/sessions/{sessionDbId}/summarize",
            post(handlers::sessions::session_summarize_legacy),
        )
        .route("/sessions/{sessionDbId}/status", get(handlers::sessions::session_status))
        .route("/sessions/{sessionDbId}", delete(handlers::sessions::session_delete))
        .route("/sessions/{sessionDbId}/complete", post(handlers::sessions::session_complete))
        .route("/api/sessions/init", post(handlers::sessions_api::api_session_init))
        .route("/api/sessions/observations", post(handlers::sessions_api::api_session_observations))
        .route("/api/sessions/summarize", post(handlers::sessions_api::api_session_summarize))
        .route("/api/pending-queue", get(handlers::queue::get_pending_queue))
        .route("/api/pending-queue/process", post(handlers::queue::process_pending_queue))
        .route("/api/pending-queue/failed", delete(handlers::queue::clear_failed_queue))
        .route("/api/pending-queue/retry-failed", post(handlers::queue::retry_failed_queue))
        .route("/api/pending-queue/all", delete(handlers::queue::clear_all_queue))
        .route("/api/processing-status", get(handlers::queue::get_processing_status))
        .route("/api/processing", post(handlers::queue::set_processing_status))
        .route("/api/settings", get(handlers::admin::get_settings))
        .route("/api/settings", post(handlers::admin::update_settings))
        .route("/api/mcp/status", get(handlers::admin::get_mcp_status))
        .route("/api/mcp/toggle", post(handlers::admin::toggle_mcp))
        .route("/api/branch/status", get(handlers::admin::get_branch_status))
        .route("/api/branch/switch", post(handlers::admin::switch_branch))
        .route("/api/branch/update", post(handlers::admin::update_branch))
        .route("/api/instructions", get(handlers::admin::get_instructions))
        .route("/api/admin/restart", post(handlers::admin::admin_restart))
        .route("/api/admin/shutdown", post(handlers::admin::admin_shutdown))
        .route("/api/admin/rebuild-embeddings", post(handlers::admin::rebuild_embeddings))
        .route("/api/unified-search", get(handlers::search::unified_search))
        .route("/api/unified-timeline", get(handlers::search::unified_timeline))
        .route("/api/decisions", get(handlers::context::get_decisions))
        .route("/api/changes", get(handlers::context::get_changes))
        .route("/api/how-it-works", get(handlers::context::get_how_it_works))
        .route("/api/context/timeline", get(handlers::context::context_timeline))
        .route("/api/context/preview", get(handlers::context::context_preview))
        .route("/api/timeline/by-query", get(handlers::context::context_timeline))
        .route("/api/search/help", get(handlers::context::search_help))
        .route("/api/knowledge", get(handlers::knowledge::list_knowledge))
        .route("/api/knowledge/search", get(handlers::knowledge::search_knowledge))
        .route(
            "/api/knowledge/{id}",
            get(handlers::knowledge::get_knowledge_by_id)
                .delete(handlers::knowledge::delete_knowledge),
        )
        .route("/api/knowledge", post(handlers::knowledge::save_knowledge))
        .route("/api/knowledge/{id}/usage", put(handlers::knowledge::record_knowledge_usage))
        .route(
            "/api/infinite/expand_summary/{id}",
            get(handlers::infinite::infinite_expand_summary),
        )
        .route("/api/infinite/time_range", get(handlers::infinite::infinite_time_range))
        .route("/api/infinite/drill_hour/{id}", get(handlers::infinite::infinite_drill_hour))
        .route("/api/infinite/drill_day/{id}", get(handlers::infinite::infinite_drill_day))
        .route("/api/infinite/search_entities", get(handlers::infinite::infinite_search_entities))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn readiness() -> (StatusCode, Json<ReadinessResponse>) {
    (StatusCode::OK, Json(ReadinessResponse { status: "ready", message: None }))
}

async fn version() -> Json<VersionResponse> {
    Json(VersionResponse { version: env!("CARGO_PKG_VERSION") })
}
