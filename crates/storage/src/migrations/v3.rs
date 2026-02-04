//! Migration v3: FTS5 for session summaries

pub const SQL: &str = r#"
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
"#;
