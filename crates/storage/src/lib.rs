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
