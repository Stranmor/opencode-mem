//! Migration v9: Vector embeddings for semantic search
//!
//! Note: This migration is optional and may fail if sqlite-vec is not available.

pub(super) const SQL: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS observations_vec USING vec0(
    embedding float[384] distance_metric=cosine
);
";
