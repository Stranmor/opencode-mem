//! Migration v10: UNIQUE constraint on session_summaries.session_id

pub const SQL: &str = r#"
-- Delete duplicates, keeping only the most recent (highest id) for each session_id
DELETE FROM session_summaries 
WHERE id NOT IN (
    SELECT MAX(id) FROM session_summaries GROUP BY session_id
);

-- Also clean up orphaned FTS entries
DELETE FROM summaries_fts WHERE rowid NOT IN (SELECT id FROM session_summaries);

-- Add unique constraint on session_id
CREATE UNIQUE INDEX IF NOT EXISTS idx_summaries_session_unique ON session_summaries(session_id);
"#;
