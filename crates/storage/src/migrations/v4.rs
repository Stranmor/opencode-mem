//! Migration v4: `pending_messages` table

pub(super) const SQL: &str = "
CREATE TABLE IF NOT EXISTS pending_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending', 'processing', 'processed', 'failed')) DEFAULT 'pending',
    tool_name TEXT,
    tool_input TEXT,
    tool_response TEXT,
    created_at_epoch INTEGER NOT NULL,
    completed_at_epoch INTEGER
);

CREATE INDEX IF NOT EXISTS idx_pending_status ON pending_messages(status);
CREATE INDEX IF NOT EXISTS idx_pending_session ON pending_messages(session_id);
";
