//! Storage layer for opencode-mem
//!
//! SQLite-based storage with FTS5 for full-text search.
//! Designed for future sqlite-vec integration for vector search.

mod migrations;
mod pending_queue;
mod storage;
#[cfg(test)]
mod tests;
mod vec_init;

pub use opencode_mem_core::StorageBackend;
pub use pending_queue::{
    PaginatedResult, PendingMessage, PendingMessageStatus, QueueStats, StorageStats,
    default_visibility_timeout_secs, max_retry_count,
};
pub use storage::Storage;
pub use vec_init::init_sqlite_vec;
