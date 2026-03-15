use crate::StorageError;
use sqlx::postgres::PgPoolOptions;
use sqlx::{ConnectOptions, PgPool};

/// Runs on a dedicated short-lived connection to avoid advisory lock leak.
/// See `pg_migrations.rs` module doc for rationale.
pub async fn run_infinite_memory_migrations(pool: &PgPool) -> Result<(), StorageError> {
    let opts = pool.connect_options();
    let url = opts.to_url_lossy().to_string();

    let migration_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect(&url)
        .await
        .map_err(|e| StorageError::Migration(format!("Migration connection: {e}")))?;

    sqlx::migrate!("./migrations_infinite")
        .run(&migration_pool)
        .await
        .map_err(|e| StorageError::Migration(format!("Infinite memory migration: {e}")))?;

    migration_pool.close().await;

    tracing::info!("Infinite Memory schema migrations completed");
    Ok(())
}
