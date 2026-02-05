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

mod migrations;
mod pending_queue;
mod storage;
#[cfg(test)]
mod tests;
mod vec_init;

pub use opencode_mem_core::StorageBackend;
pub use pending_queue::{
    default_visibility_timeout_secs, max_retry_count, PaginatedResult, PendingMessage,
    PendingMessageStatus, QueueStats, StorageStats,
};
pub use storage::Storage;
pub use vec_init::init_sqlite_vec;
