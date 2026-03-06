use crate::StorageError;
use sqlx::PgPool;

pub async fn run_infinite_memory_migrations(pool: &PgPool) -> Result<(), StorageError> {
    let migration_sqls: &[&str] = &[
        // 20240221000000_init.sql
        r#"
        CREATE TABLE IF NOT EXISTS raw_events (
            id BIGSERIAL PRIMARY KEY,
            ts TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            session_id TEXT NOT NULL,
            project TEXT,
            event_type TEXT NOT NULL,
            content JSONB NOT NULL,
            files TEXT[] NOT NULL DEFAULT '{}',
            tools TEXT[] NOT NULL DEFAULT '{}',
            summary_5min_id BIGINT,
            processing_started_at TIMESTAMPTZ,
            processing_instance_id TEXT,
            retry_count INT NOT NULL DEFAULT 0
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS summaries_5min (
            id BIGSERIAL PRIMARY KEY,
            ts_start TIMESTAMPTZ NOT NULL,
            ts_end TIMESTAMPTZ NOT NULL,
            session_id TEXT,
            project TEXT,
            content TEXT NOT NULL,
            event_count INT NOT NULL DEFAULT 0,
            entities JSONB,
            summary_hour_id BIGINT,
            processing_started_at TIMESTAMPTZ,
            processing_instance_id TEXT,
            retry_count INT NOT NULL DEFAULT 0
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS summaries_hour (
            id BIGSERIAL PRIMARY KEY,
            ts_start TIMESTAMPTZ NOT NULL,
            ts_end TIMESTAMPTZ NOT NULL,
            session_id TEXT,
            project TEXT,
            content TEXT NOT NULL,
            event_count INT NOT NULL DEFAULT 0,
            entities JSONB,
            summary_day_id BIGINT,
            processing_started_at TIMESTAMPTZ,
            processing_instance_id TEXT,
            retry_count INT NOT NULL DEFAULT 0
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS summaries_day (
            id BIGSERIAL PRIMARY KEY,
            ts_start TIMESTAMPTZ NOT NULL,
            ts_end TIMESTAMPTZ NOT NULL,
            session_id TEXT,
            project TEXT,
            content TEXT NOT NULL,
            event_count INT NOT NULL DEFAULT 0,
            entities JSONB
        )
        "#,
        "CREATE INDEX IF NOT EXISTS idx_raw_events_ts ON raw_events(ts)",
        "CREATE INDEX IF NOT EXISTS idx_raw_events_session ON raw_events(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_raw_events_summary ON raw_events(summary_5min_id)",
        "CREATE INDEX IF NOT EXISTS idx_summaries_5min_hour ON summaries_5min(summary_hour_id)",
        "CREATE INDEX IF NOT EXISTS idx_summaries_hour_day ON summaries_hour(summary_day_id)",
        // 20240221000001_add_call_id.sql
        r#"
        DO $$ BEGIN
            ALTER TABLE raw_events ADD COLUMN IF NOT EXISTS call_id TEXT;
        EXCEPTION WHEN duplicate_column THEN NULL;
        END $$
        "#,
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_raw_events_call_id ON raw_events(call_id) WHERE call_id IS NOT NULL",
        // 20240302000000_fix_ts_date.sql
        r#"
        CREATE INDEX IF NOT EXISTS idx_raw_events_unsummarized
        ON raw_events(ts ASC)
        WHERE summary_5min_id IS NULL
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_summaries_5min_unaggregated
        ON summaries_5min(ts_start ASC)
        WHERE summary_hour_id IS NULL
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_summaries_hour_unaggregated
        ON summaries_hour(ts_start ASC)
        WHERE summary_day_id IS NULL
        "#,
    ];

    for sql in migration_sqls {
        sqlx::query(sql)
            .execute(pool)
            .await
            .map_err(|e| StorageError::Migration(format!("Infinite memory migration: {e}")))?;
    }

    tracing::info!("Infinite Memory schema migrations completed");
    Ok(())
}
