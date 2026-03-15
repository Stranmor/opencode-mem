use crate::StorageError;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Runs on a dedicated short-lived pool to avoid advisory lock leak.
/// See `pg_migrations.rs` module doc for rationale.
pub async fn run_infinite_memory_migrations(pool: &PgPool) -> Result<(), StorageError> {
    let migration_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect_with(pool.connect_options().as_ref().clone())
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
