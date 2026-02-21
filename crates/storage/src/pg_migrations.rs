//! PostgreSQL schema migrations for opencode-mem storage.

use anyhow::Result;
use sqlx::PgPool;

/// Run all PostgreSQL migrations.
pub async fn run_pg_migrations(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    tracing::info!("PostgreSQL migrations completed");
    Ok(())
}
