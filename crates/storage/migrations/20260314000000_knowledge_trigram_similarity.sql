-- Enable pg_trgm extension for trigram similarity matching on knowledge titles.
-- Used by save_knowledge to find semantically similar entries before insertion.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- GIN index on lowercased titles for efficient trigram similarity queries.
CREATE INDEX IF NOT EXISTS idx_knowledge_title_trgm
    ON global_knowledge USING GIN (LOWER(title) gin_trgm_ops);
