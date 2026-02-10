//! Migration v13: Add `title_normalized` generated column and UNIQUE index for atomic dedup.

pub(super) const SQL: &str = "
ALTER TABLE observations ADD COLUMN title_normalized TEXT GENERATED ALWAYS AS (LOWER(TRIM(title))) STORED;
CREATE UNIQUE INDEX IF NOT EXISTS idx_observations_title_normalized ON observations(title_normalized);
";
