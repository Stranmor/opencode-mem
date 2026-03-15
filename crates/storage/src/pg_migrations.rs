//! PostgreSQL schema migrations for opencode-mem storage.
//!
//! Uses a dedicated short-lived connection for migrations to avoid holding
//! `pg_advisory_lock` on pool connections for the process lifetime.
//! sqlx migrations acquire a session-level advisory lock that persists until
//! the connection is closed. Running on a pooled connection means the lock
//! stays held as long as the pool keeps that connection alive, blocking any
//! other process (CLI commands, concurrent servers) from running migrations.

use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use sqlx::{ConnectOptions, PgPool};

/// Run all PostgreSQL migrations on a dedicated short-lived connection.
///
/// This avoids the advisory lock leak that occurs when running migrations
/// on a long-lived connection pool.
pub async fn run_pg_migrations(pool: &PgPool) -> Result<()> {
    // Extract connection string from the pool's connect options
    let opts = pool.connect_options();
    let url = opts.to_url_lossy().to_string();

    let migration_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect(&url)
        .await?;

    sqlx::migrate!("./migrations").run(&migration_pool).await?;

    // Explicitly close — releases the advisory lock
    migration_pool.close().await;

    tracing::info!("PostgreSQL migrations completed");
    Ok(())
}
