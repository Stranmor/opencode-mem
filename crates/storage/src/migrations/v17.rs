pub(super) const SQL: &str =
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_title_unique ON global_knowledge (title COLLATE NOCASE);";
