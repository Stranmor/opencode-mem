//! Storage layer for opencode-mem
//!
//! SQLite-based storage with FTS5 for full-text search.
//! Designed for future sqlite-vec integration for vector search.

mod sqlite;
mod migrations;

pub use sqlite::Storage;
