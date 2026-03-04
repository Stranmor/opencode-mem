-- Knowledge confidence lifecycle: archived_at column for soft-delete
ALTER TABLE global_knowledge ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ;

-- Index for efficient filtering of archived entries in search/list queries
CREATE INDEX IF NOT EXISTS idx_gk_archived_at ON global_knowledge (archived_at)
    WHERE archived_at IS NULL;
