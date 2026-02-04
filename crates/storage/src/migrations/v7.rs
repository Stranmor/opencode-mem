//! Migration v7: `retry_count` and `claimed_at` for `pending_messages`
//!
//! Note: This migration uses `add_column_if_not_exists` helper,
//! not a raw SQL batch.
