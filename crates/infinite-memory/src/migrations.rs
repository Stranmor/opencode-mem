//! Infinite Memory schema migrations — creates tables/indexes on startup.

use anyhow::Result;
use sqlx::PgPool;

/// Run all Infinite Memory schema migrations (idempotent).
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    tracing::info!("Infinite Memory schema migrations completed");
    Ok(())
}
