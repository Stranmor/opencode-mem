-- Replace IVFFlat embedding index with HNSW for better recall.
--
-- IVFFlat with lists=100 on <1000 vectors and default probes=1 searches
-- only ~1% of the vector space per query, producing poor recall and
-- returning the same irrelevant results for semantically different queries.
--
-- HNSW provides ~99% recall out of the box without tuning.
-- m=16 (connections per node), ef_construction=64 (build quality).
-- At 959 vectors the index is small (~17MB) and builds in under a second.

DROP INDEX IF EXISTS idx_obs_embedding;

CREATE INDEX idx_obs_embedding
    ON observations USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 64);
