#![allow(
    clippy::redundant_pub_crate,
    reason = "migrations module is private, pub(crate) is intentional"
)]

mod column_helpers;
mod v1;
mod v10;
mod v11;
mod v2;
mod v3;
mod v4;
mod v5;
mod v6;
mod v7;
mod v8;
mod v9;

use column_helpers::add_column_if_not_exists;
use rusqlite::Connection;

pub const SCHEMA_VERSION: i32 = 11;

#[expect(clippy::cognitive_complexity, reason = "Sequential migrations are inherently linear")]
pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "busy_timeout", 5000i32)?;

    let current_version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

    tracing::info!("Database schema version: {} (target: {})", current_version, SCHEMA_VERSION);

    if current_version < 1i32 {
        tracing::info!("Running migration v1: initial schema");
        conn.execute_batch(v1::SQL)?;
    }

    if current_version < 2i32 {
        tracing::info!("Running migration v2: FTS5 for observations");
        conn.execute_batch(v2::SQL)?;
    }

    if current_version < 3i32 {
        tracing::info!("Running migration v3: FTS5 for session summaries");
        conn.execute_batch(v3::SQL)?;
    }

    if current_version < 4i32 {
        tracing::info!("Running migration v4: pending_messages table");
        conn.execute_batch(v4::SQL)?;
    }

    if current_version < 5i32 {
        tracing::info!("Running migration v5: project column on observations");
        add_column_if_not_exists(conn, "observations", "project", "TEXT")?;
        conn.execute_batch(v5::INDEX_SQL)?;
    }

    if current_version < 6i32 {
        tracing::info!(
            "Running migration v6: files_read/files_edited columns on session_summaries"
        );
        add_column_if_not_exists(conn, "session_summaries", "files_read", "TEXT DEFAULT '[]'")?;
        add_column_if_not_exists(conn, "session_summaries", "files_edited", "TEXT DEFAULT '[]'")?;
    }

    if current_version < 7i32 {
        tracing::info!("Running migration v7: retry_count and claimed_at for pending_messages");
        add_column_if_not_exists(conn, "pending_messages", "retry_count", "INTEGER DEFAULT 0")?;
        add_column_if_not_exists(conn, "pending_messages", "claimed_at_epoch", "INTEGER")?;
    }

    if current_version < 8i32 {
        tracing::info!("Running migration v8: Global Knowledge Layer");
        conn.execute_batch(v8::SQL)?;
    }

    if current_version < 9i32 {
        tracing::info!("Running migration v9: Vector embeddings for semantic search");
        let vec_result = conn.execute_batch(v9::SQL);
        match vec_result {
            Ok(()) => tracing::info!("sqlite-vec extension loaded successfully"),
            Err(e) => tracing::warn!("sqlite-vec not available (optional): {}", e),
        }
    }

    if current_version < 10i32 {
        tracing::info!("Running migration v10: UNIQUE constraint on session_summaries.session_id");
        conn.execute_batch(v10::SQL)?;
    }

    if current_version < 11i32 {
        tracing::info!("Running migration v11: noise_level and noise_reason columns");
        add_column_if_not_exists(
            conn,
            "observations",
            v11::SQL_NOISE_LEVEL,
            v11::SQL_NOISE_LEVEL_DEF,
        )?;
        add_column_if_not_exists(
            conn,
            "observations",
            v11::SQL_NOISE_REASON,
            v11::SQL_NOISE_REASON_DEF,
        )?;
    }

    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tracing::info!("Database schema up to date (version {})", SCHEMA_VERSION);

    Ok(())
}
