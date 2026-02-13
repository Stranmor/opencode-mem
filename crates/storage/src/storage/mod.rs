//! `SQLite` storage implementation - modular structure
//!
//! Split from `sqlite_monolith.rs` for maintainability.
//! All methods are synchronous, matching the original API.

// SQLite uses i64 for counts/limits, Rust uses usize - safe conversions within DB context
#![allow(
    clippy::as_conversions,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "SQLite i64 <-> Rust usize conversions are safe within DB row counts"
)]
// Arithmetic in DB operations (pagination, counting) is bounded by DB limits
#![allow(
    clippy::arithmetic_side_effects,
    reason = "DB row counts and pagination are bounded by SQLite limits"
)]
// Absolute paths in error handling are acceptable
#![allow(clippy::absolute_paths, reason = "std paths in error handling are clear")]
// Unused results from DB operations are intentional (e.g., DELETE before INSERT)
#![allow(
    clippy::let_underscore_untyped,
    reason = "intentionally ignoring results from cleanup operations"
)]

mod embeddings;
mod knowledge;
mod knowledge_mapping;
mod observations;
mod pending;
mod prompts;
mod search;
mod sessions;
mod stats;
mod summaries;

use anyhow::Result;
use opencode_mem_core::NoiseLevel;
use opencode_mem_core::ObservationType;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr as _;

use crate::migrations;
use crate::vec_init::init_sqlite_vec;

/// Type alias for pooled connection
pub(crate) type PooledConn = PooledConnection<SqliteConnectionManager>;

/// Main storage struct wrapping `SQLite` connection pool
#[derive(Clone, Debug)]
pub struct Storage {
    pub(crate) pool: Pool<SqliteConnectionManager>,
}

/// Get a connection from the pool
pub(crate) fn get_conn(pool: &Pool<SqliteConnectionManager>) -> Result<PooledConn> {
    pool.get().map_err(|e| anyhow::anyhow!("Failed to get connection from pool: {e}"))
}

/// Parse JSON from string, converting error to rusqlite error
pub(crate) fn parse_json<T: serde::de::DeserializeOwned>(s: &str) -> rusqlite::Result<T> {
    serde_json::from_str(s).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

/// Parse `ObservationType` from a DB column value.
///
/// Handles both legacy format (JSON-quoted `"discovery"`) and
/// current format (plain string `discovery`).
pub(crate) fn parse_observation_type(s: &str) -> std::result::Result<ObservationType, String> {
    let stripped = s.trim_matches('"');
    ObservationType::from_str(stripped)
}

/// Log row read errors and filter them out
pub(crate) fn log_row_error<T>(result: rusqlite::Result<T>) -> Option<T> {
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("Row read error: {}", e);
            None
        },
    }
}

/// Map database row to `SearchResult` (6-column format: id, title, subtitle, `observation_type`, `noise_level`, score)
pub(crate) fn map_search_result(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<opencode_mem_core::SearchResult> {
    let noise_str: Option<String> = row.get(4)?;
    let noise_level = match noise_str {
        Some(s) => s.parse::<NoiseLevel>().unwrap_or_else(|_| {
            tracing::warn!(invalid_level = %s, "corrupt noise_level in DB, defaulting");
            NoiseLevel::default()
        }),
        None => NoiseLevel::default(),
    };
    Ok(opencode_mem_core::SearchResult::new(
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        parse_observation_type(&row.get::<_, String>(3)?)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?,
        noise_level,
        row.get(5)?,
    ))
}

/// Map database row to `SearchResult` (5-column format with default score=1.0)
pub(crate) fn map_search_result_default_score(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<opencode_mem_core::SearchResult> {
    let noise_str: Option<String> = row.get(4)?;
    let noise_level = match noise_str {
        Some(s) => s.parse::<NoiseLevel>().unwrap_or_else(|_| {
            tracing::warn!(invalid_level = %s, "corrupt noise_level in DB, defaulting");
            NoiseLevel::default()
        }),
        None => NoiseLevel::default(),
    };
    Ok(opencode_mem_core::SearchResult::new(
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        parse_observation_type(&row.get::<_, String>(3)?)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?,
        noise_level,
        1.0,
    ))
}

/// Escape special characters for LIKE pattern matching
pub(crate) fn escape_like_pattern(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// Coerce a reference to `ToSql` trait object (avoids trivial cast lint)
pub(crate) fn coerce_to_sql<T: rusqlite::ToSql>(val: &T) -> &dyn rusqlite::ToSql {
    val
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

pub(crate) fn union_dedup(existing: &[String], newer: &[String]) -> Vec<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut result = Vec::with_capacity(existing.len().saturating_add(newer.len()));
    for item in existing.iter().chain(newer.iter()) {
        if seen.insert(item.as_str()) {
            result.push(item.clone());
        }
    }
    result
}

/// Custom connection initializer for sqlite-vec and concurrency settings
fn init_connection(conn: &mut Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "PRAGMA busy_timeout = 30000;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = FULL;",
    )?;
    Ok(())
}

fn db_pool_size() -> u32 {
    opencode_mem_core::env_parse_with_default("OPENCODE_MEM_DB_POOL_SIZE", 8)
}

impl Storage {
    /// Create new storage instance with `SQLite` connection pool
    pub fn new(db_path: &Path) -> Result<Self> {
        init_sqlite_vec();

        let manager = SqliteConnectionManager::file(db_path).with_init(init_connection);

        let pool_size = db_pool_size();
        let pool = Pool::builder().max_size(pool_size).build(manager)?;

        // Run migrations on first connection
        let conn = pool.get()?;
        migrations::run_migrations(&conn)?;
        drop(conn);

        tracing::info!(pool_size = pool_size, "Storage initialized with connection pool");

        Ok(Self { pool })
    }
}
