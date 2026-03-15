//! PostgreSQL schema migrations for opencode-mem storage.
//!
//! Uses a dedicated short-lived connection for migrations to avoid holding
//! `pg_advisory_lock` on pool connections for the process lifetime.
//! sqlx migrations acquire a session-level advisory lock that persists until
//! the connection is closed. Running on a pooled connection means the lock
//! stays held as long as the pool keeps that connection alive, blocking any
//! other process (CLI commands, concurrent servers) from running migrations.

use anyhow::Result;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Run all PostgreSQL migrations on a dedicated short-lived pool.
///
/// Uses a single-connection pool (not `&mut PgConnection`) because
/// `sqlx::migrate!().run()` requires `impl Acquire` with `'static`
/// bounds when called from `tokio::spawn`. The pool is closed after
/// migrations, releasing the session-level advisory lock.
pub async fn run_pg_migrations(pool: &PgPool) -> Result<()> {
    let migration_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect_with(pool.connect_options().as_ref().clone())
        .await?;

    sqlx::migrate!("./migrations").run(&migration_pool).await?;

    migration_pool.close().await;

    tracing::info!("PostgreSQL migrations completed");
    Ok(())
}
