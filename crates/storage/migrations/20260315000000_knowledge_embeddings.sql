-- Add embedding column to global_knowledge for semantic dedup.
-- Uses the same pgvector type as observations.embedding.
ALTER TABLE global_knowledge ADD COLUMN IF NOT EXISTS embedding vector(1024);

-- Index for cosine distance search (ivfflat with cosine ops).
-- Uses lists=10 because global_knowledge is small (~100-500 entries).
CREATE INDEX IF NOT EXISTS idx_gk_embedding
    ON global_knowledge USING ivfflat (embedding vector_cosine_ops)
    WITH (lists = 10);
