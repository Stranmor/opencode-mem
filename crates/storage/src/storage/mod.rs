//! SQLite storage implementation - modular structure
//!
//! Split from sqlite_monolith.rs for maintainability.
//! All methods are synchronous, matching the original API.

mod backend;
mod embeddings;
mod knowledge;
mod observations;
mod pending;
mod prompts;
mod search;
mod sessions;
mod stats;
mod summaries;

use anyhow::Result;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::path::Path;

use crate::migrations;
use crate::vec_init::init_sqlite_vec;

/// Type alias for pooled connection
pub type PooledConn = PooledConnection<SqliteConnectionManager>;

/// Main storage struct wrapping SQLite connection pool
#[derive(Clone)]
pub struct Storage {
    pub(crate) pool: Pool<SqliteConnectionManager>,
}

/// Get a connection from the pool
pub(crate) fn get_conn(pool: &Pool<SqliteConnectionManager>) -> Result<PooledConn> {
    pool.get()
        .map_err(|e| anyhow::anyhow!("Failed to get connection from pool: {}", e))
}

/// Parse JSON from string, converting error to rusqlite error
pub(crate) fn parse_json<T: serde::de::DeserializeOwned>(s: &str) -> rusqlite::Result<T> {
    serde_json::from_str(s).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

/// Log row read errors and filter them out
pub(crate) fn log_row_error<T>(result: rusqlite::Result<T>) -> Option<T> {
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("Row read error: {}", e);
            None
        }
    }
}

/// Map database row to SearchResult (5-column format: id, title, subtitle, observation_type, score)
pub(crate) fn map_search_result(
    row: &rusqlite::Row,
) -> rusqlite::Result<opencode_mem_core::SearchResult> {
    Ok(opencode_mem_core::SearchResult {
        id: row.get(0)?,
        title: row.get(1)?,
        subtitle: row.get(2)?,
        observation_type: parse_json(&row.get::<_, String>(3)?)?,
        score: row.get(4)?,
    })
}

/// Map database row to SearchResult (4-column format with default score=1.0)
pub(crate) fn map_search_result_default_score(
    row: &rusqlite::Row,
) -> rusqlite::Result<opencode_mem_core::SearchResult> {
    Ok(opencode_mem_core::SearchResult {
        id: row.get(0)?,
        title: row.get(1)?,
        subtitle: row.get(2)?,
        observation_type: parse_json(&row.get::<_, String>(3)?)?,
        score: 1.0,
    })
}

/// Escape special characters for LIKE pattern matching
pub(crate) fn escape_like_pattern(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Build FTS5 query from whitespace-separated words
/// Each word becomes a quoted prefix match, joined with AND
pub(crate) fn build_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| format!("\"{}\"*", word.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// Custom connection initializer for sqlite-vec
fn init_connection(_conn: &mut Connection) -> Result<(), rusqlite::Error> {
    init_sqlite_vec();
    Ok(())
}

impl Storage {
    /// Create new storage instance with SQLite connection pool
    pub fn new(db_path: &Path) -> Result<Self> {
        init_sqlite_vec();

        let manager = SqliteConnectionManager::file(db_path).with_init(init_connection);

        let pool = Pool::builder().max_size(8).build(manager)?;

        // Run migrations on first connection
        let conn = pool.get()?;
        migrations::run_migrations(&conn)?;
        drop(conn);

        Ok(Self { pool })
    }
}
