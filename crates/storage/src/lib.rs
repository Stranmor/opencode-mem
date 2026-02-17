//! Storage layer for opencode-mem
//!
//! `SQLite`-based storage with FTS5 for full-text search.
//! Designed for future sqlite-vec integration for vector search.

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
#![allow(clippy::undocumented_unsafe_blocks, reason = "FFI safety is documented in vec_init")]

pub mod backend;
#[cfg(feature = "sqlite")]
mod migrations;
mod pending_queue;
#[cfg(feature = "postgres")]
pub mod pg_migrations;
#[cfg(feature = "postgres")]
pub mod pg_storage;
#[cfg(feature = "sqlite")]
mod sqlite_async;
#[cfg(feature = "sqlite")]
mod storage;
#[cfg(test)]
mod tests;
pub mod traits;
#[cfg(feature = "sqlite")]
mod vec_init;

pub use backend::StorageBackend;
pub use pending_queue::{
    default_visibility_timeout_secs, max_retry_count, PaginatedResult, PendingMessage,
    PendingMessageStatus, QueueStats, StorageStats,
};
#[cfg(feature = "postgres")]
pub use pg_storage::PgStorage;
#[cfg(feature = "sqlite")]
pub use storage::Storage;
pub use traits::{
    EmbeddingStore, InjectionStore, KnowledgeStore, ObservationStore, PendingQueueStore,
    PromptStore, SearchStore, SessionStore, StatsStore, SummaryStore,
};
#[cfg(feature = "sqlite")]
pub use vec_init::init_sqlite_vec;
