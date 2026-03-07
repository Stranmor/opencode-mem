//! HTTP API server for opencode-mem.

#![allow(missing_docs, reason = "Internal crate with self-explanatory API")]
#![allow(unreachable_pub, reason = "pub items are re-exported")]
#![allow(clippy::absolute_paths, reason = "Explicit paths for clarity")]
#![allow(unused_results, reason = "Some results are intentionally ignored")]
#![allow(
    clippy::arithmetic_side_effects,
    reason = "Arithmetic is safe in context"
)]
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
#![allow(
    clippy::shadow_unrelated,
    reason = "Shadowing in async blocks is idiomatic"
)]
#![allow(clippy::exhaustive_structs, reason = "HTTP types are stable")]
#![allow(
    clippy::single_call_fn,
    reason = "Helper functions improve readability"
)]

pub mod api_error;
mod api_types;
mod blocking;
mod handlers;
mod query_types;
mod response_types;
mod routes;
mod viewer;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use tokio::sync::{RwLock, Semaphore, broadcast};

use opencode_mem_core::AppConfig;
use opencode_mem_service::{
    InfiniteMemoryService, KnowledgeService, ObservationService, QueueService, SearchService,
    SessionService,
};

pub use api_types::{HealthResponse, ReadinessResponse, Settings, VersionResponse};
pub use handlers::queue_processor::{run_startup_recovery, start_background_processor};
pub use routes::create_router;

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
    pub infinite_mem: Option<Arc<InfiniteMemoryService>>,
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
    /// In-memory buffer for write operations during degraded mode
    pub pending_writes: Arc<opencode_mem_service::PendingWriteQueue>,
    pub shutdown_tx: tokio::sync::broadcast::Sender<bool>,
    pub started_at: Instant,
    pub config: Arc<AppConfig>,
}
