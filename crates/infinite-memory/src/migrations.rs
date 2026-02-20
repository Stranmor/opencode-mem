//! Infinite Memory schema migrations — creates tables/indexes on startup.

use anyhow::Result;
use sqlx::PgPool;

/// Run all Infinite Memory schema migrations (idempotent).
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    // raw_events table
    sqlx::query(
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
            processing_instance_id TEXT
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_raw_events_ts ON raw_events(ts DESC)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_raw_events_session ON raw_events(session_id)")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_raw_events_project ON raw_events(project) WHERE project IS NOT NULL",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_raw_events_unsummarized ON raw_events(ts ASC) WHERE summary_5min_id IS NULL",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_raw_events_processing ON raw_events(processing_started_at) WHERE processing_started_at IS NOT NULL",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_raw_events_content ON raw_events USING GIN(content)",
    )
    .execute(pool)
    .await?;

    // summaries_5min table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS summaries_5min (
            id BIGSERIAL PRIMARY KEY,
            ts_start TIMESTAMPTZ NOT NULL,
            ts_end TIMESTAMPTZ NOT NULL,
            session_id TEXT,
            project TEXT,
            content TEXT NOT NULL,
            event_count INTEGER NOT NULL,
            entities JSONB,
            summary_hour_id BIGINT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_summaries_5min_ts ON summaries_5min(ts_start)")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_summaries_5min_session ON summaries_5min(session_id) WHERE session_id IS NOT NULL",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_summaries_5min_unaggregated ON summaries_5min(ts_start ASC) WHERE summary_hour_id IS NULL",
    )
    .execute(pool)
    .await?;

    // summaries_hour table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS summaries_hour (
            id BIGSERIAL PRIMARY KEY,
            ts_start TIMESTAMPTZ NOT NULL,
            ts_end TIMESTAMPTZ NOT NULL,
            session_id TEXT,
            project TEXT,
            content TEXT NOT NULL,
            event_count INTEGER NOT NULL,
            entities JSONB,
            summary_day_id BIGINT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_summaries_hour_ts ON summaries_hour(ts_start)")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_summaries_hour_session ON summaries_hour(session_id) WHERE session_id IS NOT NULL",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_summaries_hour_unaggregated ON summaries_hour(ts_start ASC) WHERE summary_day_id IS NULL",
    )
    .execute(pool)
    .await?;

    // summaries_day table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS summaries_day (
            id BIGSERIAL PRIMARY KEY,
            ts_start TIMESTAMPTZ NOT NULL,
            ts_end TIMESTAMPTZ NOT NULL,
            session_id TEXT,
            project TEXT,
            content TEXT NOT NULL,
            event_count INTEGER NOT NULL,
            entities JSONB,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_summaries_day_ts ON summaries_day(ts_start)")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_summaries_day_session ON summaries_day(session_id) WHERE session_id IS NOT NULL",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "ALTER TABLE raw_events ADD COLUMN IF NOT EXISTS files TEXT[] NOT NULL DEFAULT '{}'",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "ALTER TABLE raw_events ADD COLUMN IF NOT EXISTS tools TEXT[] NOT NULL DEFAULT '{}'",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "ALTER TABLE raw_events ADD COLUMN IF NOT EXISTS processing_started_at TIMESTAMPTZ",
    )
    .execute(pool)
    .await?;
    sqlx::query("ALTER TABLE raw_events ADD COLUMN IF NOT EXISTS processing_instance_id TEXT")
        .execute(pool)
        .await?;

    sqlx::query(
        "ALTER TABLE summaries_5min ADD COLUMN IF NOT EXISTS processing_started_at TIMESTAMPTZ",
    )
    .execute(pool)
    .await?;
    sqlx::query("ALTER TABLE summaries_5min ADD COLUMN IF NOT EXISTS processing_instance_id TEXT")
        .execute(pool)
        .await?;
    sqlx::query(
        "ALTER TABLE summaries_hour ADD COLUMN IF NOT EXISTS processing_started_at TIMESTAMPTZ",
    )
    .execute(pool)
    .await?;
    sqlx::query("ALTER TABLE summaries_hour ADD COLUMN IF NOT EXISTS processing_instance_id TEXT")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_summaries_5min_processing ON summaries_5min(processing_started_at) WHERE processing_started_at IS NOT NULL").execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_summaries_hour_processing ON summaries_hour(processing_started_at) WHERE processing_started_at IS NOT NULL").execute(pool).await?;

    sqlx::query("ALTER TABLE summaries_5min ADD COLUMN IF NOT EXISTS entities JSONB")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE summaries_hour ADD COLUMN IF NOT EXISTS entities JSONB")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE summaries_day ADD COLUMN IF NOT EXISTS entities JSONB")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_summaries_5min_entities ON summaries_5min USING GIN(entities)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_summaries_hour_entities ON summaries_hour USING GIN(entities)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_summaries_day_entities ON summaries_day USING GIN(entities)")
        .execute(pool)
        .await?;

    // Foreign keys (idempotent creation)
    sqlx::query(
        r#"
        DO $$ BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM pg_constraint WHERE conname = 'fk_raw_events_summary_5min'
            ) THEN
                ALTER TABLE raw_events
                    ADD CONSTRAINT fk_raw_events_summary_5min
                    FOREIGN KEY (summary_5min_id) REFERENCES summaries_5min(id)
                    ON DELETE SET NULL;
            END IF;
        END $$;
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        DO $$ BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM pg_constraint WHERE conname = 'fk_summaries_5min_hour'
            ) THEN
                ALTER TABLE summaries_5min
                    ADD CONSTRAINT fk_summaries_5min_hour
                    FOREIGN KEY (summary_hour_id) REFERENCES summaries_hour(id)
                    ON DELETE SET NULL;
            END IF;
        END $$;
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        DO $$ BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM pg_constraint WHERE conname = 'fk_summaries_hour_day'
            ) THEN
                ALTER TABLE summaries_hour
                    ADD CONSTRAINT fk_summaries_hour_day
                    FOREIGN KEY (summary_day_id) REFERENCES summaries_day(id)
                    ON DELETE SET NULL;
            END IF;
        END $$;
        "#,
    )
    .execute(pool)
    .await?;

    tracing::info!("Infinite Memory schema migrations completed");
    Ok(())
}
