//! Database migrations

use rusqlite::Connection;

pub const SCHEMA_VERSION: i32 = 3;

pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    let current_version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

    tracing::info!(
        "Database schema version: {} (target: {})",
        current_version,
        SCHEMA_VERSION
    );

    if current_version < 1 {
        tracing::info!("Running migration v1: initial schema");
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                content_session_id TEXT NOT NULL,
                memory_session_id TEXT,
                project TEXT NOT NULL,
                user_prompt TEXT,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                prompt_counter INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS observations (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                observation_type TEXT NOT NULL,
                title TEXT NOT NULL,
                subtitle TEXT,
                narrative TEXT,
                facts TEXT,
                concepts TEXT,
                files_read TEXT,
                files_modified TEXT,
                keywords TEXT,
                prompt_number INTEGER,
                discovery_tokens INTEGER,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS session_summaries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                project TEXT NOT NULL,
                request TEXT,
                investigated TEXT,
                learned TEXT,
                completed TEXT,
                next_steps TEXT,
                notes TEXT,
                prompt_number INTEGER,
                discovery_tokens INTEGER,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS user_prompts (
                id TEXT PRIMARY KEY,
                content_session_id TEXT NOT NULL,
                prompt_number INTEGER NOT NULL,
                prompt_text TEXT NOT NULL,
                project TEXT,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_observations_session ON observations(session_id);
            CREATE INDEX IF NOT EXISTS idx_observations_created ON observations(created_at);
            CREATE INDEX IF NOT EXISTS idx_sessions_content ON sessions(content_session_id);
            CREATE INDEX IF NOT EXISTS idx_summaries_session ON session_summaries(session_id);
            "#,
        )?;
    }

    if current_version < 2 {
        tracing::info!("Running migration v2: FTS5 for observations");
        conn.execute_batch(
            r#"
            DROP TABLE IF EXISTS observations_fts;
            
            CREATE VIRTUAL TABLE observations_fts USING fts5(
                title, subtitle, narrative, facts, keywords,
                content='observations',
                content_rowid='rowid'
            );

            DROP TRIGGER IF EXISTS observations_ai;
            CREATE TRIGGER observations_ai AFTER INSERT ON observations BEGIN
                INSERT INTO observations_fts(rowid, title, subtitle, narrative, facts, keywords)
                VALUES (new.rowid, new.title, new.subtitle, new.narrative, new.facts, new.keywords);
            END;

            INSERT INTO observations_fts(rowid, title, subtitle, narrative, facts, keywords)
            SELECT rowid, title, subtitle, narrative, facts, keywords FROM observations;
            "#,
        )?;
    }

    if current_version < 3 {
        tracing::info!("Running migration v3: FTS5 for session summaries");
        conn.execute_batch(
            r#"
            DROP TABLE IF EXISTS summaries_fts;
            
            CREATE VIRTUAL TABLE summaries_fts USING fts5(
                request, investigated, learned, completed, next_steps, notes,
                content='session_summaries',
                content_rowid='id'
            );

            DROP TRIGGER IF EXISTS summaries_ai;
            CREATE TRIGGER summaries_ai AFTER INSERT ON session_summaries BEGIN
                INSERT INTO summaries_fts(rowid, request, investigated, learned, completed, next_steps, notes)
                VALUES (new.id, new.request, new.investigated, new.learned, new.completed, new.next_steps, new.notes);
            END;

            INSERT INTO summaries_fts(rowid, request, investigated, learned, completed, next_steps, notes)
            SELECT id, request, investigated, learned, completed, next_steps, notes FROM session_summaries;
            "#,
        )?;
    }

    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tracing::info!("Database schema up to date (version {})", SCHEMA_VERSION);

    Ok(())
}
