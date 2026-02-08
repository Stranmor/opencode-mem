//! Migration v12: Add `project` column to `pending_messages`

pub(super) const COLUMN_NAME: &str = "project";
pub(super) const COLUMN_DEF: &str = "TEXT";
