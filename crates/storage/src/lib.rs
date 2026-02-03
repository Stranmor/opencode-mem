//! Storage layer for opencode-mem
//!
//! SQLite-based storage with FTS5 for full-text search.
//! Designed for future sqlite-vec integration for vector search.

mod migrations;
mod sqlite_monolith;
#[cfg(test)]
mod tests;
mod types;

pub use sqlite_monolith::Storage;
pub use types::{PaginatedResult, StorageStats};
