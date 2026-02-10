//! Migration v13: UNIQUE expression index on `LOWER(TRIM(title))` for atomic dedup.
//!
//! SQLite cannot `ALTER TABLE ADD COLUMN ... STORED`, so we use an expression
//! index directly — no generated column needed.

pub(super) const SQL: &str = "
CREATE UNIQUE INDEX IF NOT EXISTS idx_observations_title_normalized ON observations(LOWER(TRIM(title)));
";
