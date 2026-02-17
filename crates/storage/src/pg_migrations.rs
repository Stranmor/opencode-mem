//! PostgreSQL schema migrations for opencode-mem storage.

use anyhow::Result;
use sqlx::PgPool;

/// Run all PostgreSQL migrations.
pub async fn run_pg_migrations(pool: &PgPool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS observations (
            id TEXT PRIMARY KEY,
            session_id TEXT,
            project TEXT,
            observation_type TEXT NOT NULL,
            title TEXT NOT NULL,
            title_normalized TEXT GENERATED ALWAYS AS (LOWER(TRIM(title))) STORED,
            subtitle TEXT,
            narrative TEXT,
            facts JSONB NOT NULL DEFAULT '[]',
            concepts JSONB NOT NULL DEFAULT '[]',
            files_read JSONB NOT NULL DEFAULT '[]',
            files_modified JSONB NOT NULL DEFAULT '[]',
            keywords JSONB NOT NULL DEFAULT '[]',
            prompt_number INTEGER,
            discovery_tokens INTEGER,
            noise_level TEXT DEFAULT 'normal',
            noise_reason TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_obs_title_norm ON observations (title_normalized)",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_obs_session ON observations (session_id)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_obs_project ON observations (project)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_obs_created ON observations (created_at DESC)")
        .execute(pool)
        .await?;

    // Full-text search: tsvector column + GIN index
    sqlx::query(
        r#"
        DO $$ BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_name = 'observations' AND column_name = 'search_vec'
            ) THEN
                ALTER TABLE observations ADD COLUMN search_vec tsvector
                    GENERATED ALWAYS AS (
                        setweight(to_tsvector('english', COALESCE(title, '')), 'A') ||
                        setweight(to_tsvector('english', COALESCE(subtitle, '')), 'B') ||
                        setweight(to_tsvector('english', COALESCE(narrative, '')), 'C')
                    ) STORED;
            END IF;
        END $$
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_obs_search_vec ON observations USING GIN (search_vec)",
    )
    .execute(pool)
    .await?;

    // pgvector extension + embedding column
    sqlx::query("CREATE EXTENSION IF NOT EXISTS vector").execute(pool).await?;

    sqlx::query(
        r#"
        DO $$ BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_name = 'observations' AND column_name = 'embedding'
            ) THEN
                ALTER TABLE observations ADD COLUMN embedding vector(1024);
            END IF;
        END $$
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_obs_embedding ON observations USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100)",
    )
    .execute(pool)
    .await
    .ok(); // May fail if < 100 rows; that's fine

    // Sessions
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            content_session_id TEXT,
            memory_session_id TEXT,
            project TEXT,
            user_prompt TEXT,
            started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            ended_at TIMESTAMPTZ,
            status TEXT NOT NULL DEFAULT 'active',
            prompt_counter INTEGER NOT NULL DEFAULT 0
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_sess_content ON sessions (content_session_id)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_sess_status ON sessions (status)")
        .execute(pool)
        .await?;

    // Global knowledge
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS global_knowledge (
            id TEXT PRIMARY KEY,
            knowledge_type TEXT NOT NULL,
            title TEXT NOT NULL,
            description TEXT,
            instructions TEXT,
            triggers JSONB NOT NULL DEFAULT '[]',
            source_projects JSONB NOT NULL DEFAULT '[]',
            source_observations JSONB NOT NULL DEFAULT '[]',
            confidence DOUBLE PRECISION NOT NULL DEFAULT 0.5,
            usage_count INTEGER NOT NULL DEFAULT 0,
            last_used_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        DO $$ BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_name = 'global_knowledge' AND column_name = 'search_vec'
            ) THEN
                ALTER TABLE global_knowledge ADD COLUMN search_vec tsvector
                    GENERATED ALWAYS AS (
                        setweight(to_tsvector('english', COALESCE(title, '')), 'A') ||
                        setweight(to_tsvector('english', COALESCE(description, '')), 'B') ||
                        setweight(to_tsvector('english', COALESCE(instructions, '')), 'C')
                    ) STORED;
            END IF;
        END $$
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_gk_search_vec ON global_knowledge USING GIN (search_vec)",
    )
    .execute(pool)
    .await?;

    // Session summaries
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS session_summaries (
            session_id TEXT PRIMARY KEY,
            project TEXT,
            request TEXT,
            investigated TEXT,
            learned TEXT,
            completed TEXT,
            next_steps TEXT,
            notes TEXT,
            files_read JSONB NOT NULL DEFAULT '[]',
            files_edited JSONB NOT NULL DEFAULT '[]',
            prompt_number INTEGER,
            discovery_tokens INTEGER,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        DO $$ BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_name = 'session_summaries' AND column_name = 'search_vec'
            ) THEN
                ALTER TABLE session_summaries ADD COLUMN search_vec tsvector
                    GENERATED ALWAYS AS (
                        setweight(to_tsvector('english', COALESCE(request, '')), 'A') ||
                        setweight(to_tsvector('english', COALESCE(learned, '')), 'B') ||
                        setweight(to_tsvector('english', COALESCE(completed, '')), 'C') ||
                        to_tsvector('english', COALESCE(investigated, '')) ||
                        to_tsvector('english', COALESCE(next_steps, '')) ||
                        to_tsvector('english', COALESCE(notes, ''))
                    ) STORED;
            END IF;
        END $$
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_ss_search_vec ON session_summaries USING GIN (search_vec)",
    )
    .execute(pool)
    .await?;

    // Pending messages queue
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS pending_messages (
            id BIGSERIAL PRIMARY KEY,
            session_id TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            tool_name TEXT,
            tool_input TEXT,
            tool_response TEXT,
            retry_count INTEGER NOT NULL DEFAULT 0,
            created_at_epoch BIGINT NOT NULL,
            claimed_at_epoch BIGINT,
            completed_at_epoch BIGINT,
            project TEXT
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_pm_status ON pending_messages (status)")
        .execute(pool)
        .await?;

    // User prompts
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS user_prompts (
            id TEXT PRIMARY KEY,
            content_session_id TEXT,
            prompt_number INTEGER,
            prompt_text TEXT,
            project TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_up_project ON user_prompts (project)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_up_created ON user_prompts (created_at DESC)")
        .execute(pool)
        .await?;

    // Upgrade embedding column from vector(384) to vector(1024) for BGE-M3.
    // Only runs when column is still 384d (checked via pg_attribute.atttypmod).
    // Must drop ivfflat index first, NULL data, ALTER type, then recreate index.
    // Embeddings will be regenerated via `backfill-embeddings` command.
    sqlx::query(
        r#"
        DO $$ BEGIN
            IF EXISTS (
                SELECT 1 FROM pg_attribute
                WHERE attrelid = 'observations'::regclass
                AND attname = 'embedding'
                AND atttypmod = 384
            ) THEN
                DROP INDEX IF EXISTS idx_obs_embedding;
                UPDATE observations SET embedding = NULL WHERE embedding IS NOT NULL;
                ALTER TABLE observations ALTER COLUMN embedding TYPE vector(1024);
                CREATE INDEX idx_obs_embedding ON observations
                    USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);
            END IF;
        END $$
        "#,
    )
    .execute(pool)
    .await
    .ok(); // May fail if < 100 rows for ivfflat; that's fine

    // Injected observations tracking for injection-aware dedup
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS injected_observations (
            session_id TEXT NOT NULL,
            observation_id TEXT NOT NULL,
            injected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (session_id, observation_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_injected_obs_session ON injected_observations(session_id)",
    )
    .execute(pool)
    .await?;

    tracing::info!("PostgreSQL migrations completed");
    Ok(())
}
