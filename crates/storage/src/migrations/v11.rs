//! Migration v11: Add `noise_level` and `noise_reason` columns to observations

pub(super) const SQL_NOISE_LEVEL: &str = "noise_level";
pub(super) const SQL_NOISE_REASON: &str = "noise_reason";
pub(super) const SQL_NOISE_LEVEL_DEF: &str = "TEXT DEFAULT 'medium'";
pub(super) const SQL_NOISE_REASON_DEF: &str = "TEXT";
