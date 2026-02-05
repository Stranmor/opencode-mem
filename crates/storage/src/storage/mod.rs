//! `SQLite` storage implementation - modular structure
//!
//! Split from `sqlite_monolith.rs` for maintainability.
//! All methods are synchronous, matching the original API.

// Storage is internal infrastructure - detailed error docs not needed for each DB operation
#![allow(
    clippy::missing_errors_doc,
    reason = "internal storage layer - errors are self-explanatory DB/IO failures"
)]
// SQLite uses i64 for counts/limits, Rust uses usize - safe conversions within DB context
#![allow(
    clippy::as_conversions,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "SQLite i64 <-> Rust usize conversions are safe within DB row counts"
)]
// Floating point is used for search scores - acceptable for ranking
#![allow(clippy::float_arithmetic, reason = "search relevance scores use floating point by design")]
// Arithmetic in DB operations (pagination, counting) is bounded by DB limits
#![allow(
    clippy::arithmetic_side_effects,
    reason = "DB row counts and pagination are bounded by SQLite limits"
)]
// Numeric literals in DB code are clear from context
#![allow(
    clippy::default_numeric_fallback,
    reason = "numeric types are clear from DB schema context"
)]
// Storage module is private, pub(crate) items are effectively pub within this module
#![allow(
    clippy::redundant_pub_crate,
    reason = "storage module is private, pub(crate) is intentional for internal API"
)]
// Long functions in search are acceptable - they're cohesive DB operations
#![allow(clippy::too_many_lines, reason = "search functions are cohesive DB operations")]
// Absolute paths in error handling are acceptable
#![allow(clippy::absolute_paths, reason = "std paths in error handling are clear")]
// Multiple impl blocks organize code by functionality
#![allow(
    clippy::multiple_inherent_impl,
    reason = "impl blocks are split by functionality across files"
)]
// Unused results from DB operations are intentional (e.g., DELETE before INSERT)
#![allow(
    clippy::let_underscore_must_use,
    clippy::let_underscore_untyped,
    reason = "intentionally ignoring results from cleanup operations"
)]
// Sync methods have same names as async trait methods - this is intentional design
#![allow(
    clippy::same_name_method,
    reason = "sync methods wrap async trait methods with same names"
)]
// Pattern matching on references is acceptable
#![allow(clippy::pattern_type_mismatch, reason = "iterator patterns on references are clear")]

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
use opencode_mem_core::NoiseLevel;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
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
pub fn get_conn(pool: &Pool<SqliteConnectionManager>) -> Result<PooledConn> {
    pool.get().map_err(|e| anyhow::anyhow!("Failed to get connection from pool: {e}"))
}

/// Parse JSON from string, converting error to rusqlite error
pub fn parse_json<T: serde::de::DeserializeOwned>(s: &str) -> rusqlite::Result<T> {
    serde_json::from_str(s).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

/// Log row read errors and filter them out
pub fn log_row_error<T>(result: rusqlite::Result<T>) -> Option<T> {
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("Row read error: {}", e);
            None
        },
    }
}

/// Map database row to `SearchResult` (6-column format: id, title, subtitle, `observation_type`, `noise_level`, score)
pub fn map_search_result(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<opencode_mem_core::SearchResult> {
    let noise_str: Option<String> = row.get(4)?;
    let noise_level = noise_str.and_then(|s| NoiseLevel::from_str(&s).ok()).unwrap_or_default();
    Ok(opencode_mem_core::SearchResult::new(
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        parse_json(&row.get::<_, String>(3)?)?,
        noise_level,
        row.get(5)?,
    ))
}

/// Map database row to `SearchResult` (5-column format with default score=1.0)
pub fn map_search_result_default_score(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<opencode_mem_core::SearchResult> {
    let noise_str: Option<String> = row.get(4)?;
    let noise_level = noise_str.and_then(|s| NoiseLevel::from_str(&s).ok()).unwrap_or_default();
    Ok(opencode_mem_core::SearchResult::new(
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        parse_json(&row.get::<_, String>(3)?)?,
        noise_level,
        1.0,
    ))
}

/// Escape special characters for LIKE pattern matching
pub fn escape_like_pattern(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// Coerce a reference to `ToSql` trait object (avoids trivial cast lint)
pub fn coerce_to_sql<T: rusqlite::ToSql>(val: &T) -> &dyn rusqlite::ToSql {
    val
}

/// Build FTS5 query from whitespace-separated words
/// Each word becomes a quoted prefix match, joined with AND
pub fn build_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| format!("\"{}\"*", word.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// Custom connection initializer for sqlite-vec and concurrency settings
fn init_connection(conn: &mut Connection) -> Result<(), rusqlite::Error> {
    init_sqlite_vec();
    conn.execute_batch(
        "PRAGMA busy_timeout = 30000;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;",
    )?;
    Ok(())
}

fn db_pool_size() -> u32 {
    std::env::var("OPENCODE_MEM_DB_POOL_SIZE").ok().and_then(|v| v.parse().ok()).unwrap_or(8)
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
