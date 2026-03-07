use crate::StorageError;
use sqlx::PgPool;

pub async fn run_infinite_memory_migrations(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::migrate!("./migrations_infinite")
        .run(pool)
        .await
        .map_err(|e| StorageError::Migration(format!("Infinite memory migration: {e}")))?;

    tracing::info!("Infinite Memory schema migrations completed");
    Ok(())
}
