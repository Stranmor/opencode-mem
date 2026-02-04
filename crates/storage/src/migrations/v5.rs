//! Migration v5: project column on observations
//!
//! Note: This migration uses `add_column_if_not_exists` helper,
//! not a raw SQL batch.

pub(super) const INDEX_SQL: &str =
    "CREATE INDEX IF NOT EXISTS idx_observations_project ON observations(project);";
