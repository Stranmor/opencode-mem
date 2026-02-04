//! Migration v6: `files_read/files_edited` columns on `session_summaries`
//!
//! Note: This migration uses `add_column_if_not_exists` helper,
//! not a raw SQL batch.
