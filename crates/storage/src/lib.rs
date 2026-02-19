//! Storage layer for opencode-mem
//!
//! PostgreSQL-based storage with tsvector + GIN for full-text search
//! and pgvector for vector similarity search.

#![allow(
    unused_results,
    reason = "SQL execute() returns row count which is often unused in INSERT/UPDATE operations"
)]
#![allow(
    unreachable_pub,
    reason = "pub items in private modules are re-exported via pub use in lib.rs"
)]
#![allow(clippy::missing_docs_in_private_items, reason = "Internal storage modules")]
#![allow(clippy::implicit_return, reason = "Implicit return is idiomatic Rust")]
#![allow(clippy::question_mark_used, reason = "? operator is idiomatic Rust")]
#![allow(clippy::min_ident_chars, reason = "Short closure params are idiomatic")]
#![allow(clippy::shadow_reuse, reason = "Shadowing for owned copies is idiomatic")]
#![allow(clippy::single_call_fn, reason = "Helper functions improve readability")]

pub mod backend;
pub mod error;
mod pending_queue;
pub mod pg_migrations;
pub mod pg_storage;
#[cfg(test)]
mod tests;
pub mod traits;

pub use backend::StorageBackend;
pub use error::StorageError;
pub use pending_queue::{
    default_visibility_timeout_secs, max_retry_count, PaginatedResult, PendingMessage,
    PendingMessageStatus, QueueStats, StorageStats,
};
pub use pg_storage::PgStorage;
pub use traits::{
    EmbeddingStore, InjectionStore, KnowledgeStore, ObservationStore, PendingQueueStore,
    PromptStore, SearchStore, SessionStore, StatsStore, SummaryStore,
};
