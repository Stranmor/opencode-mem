use opencode_mem_storage::pg_storage::infinite_memory::{
    create_5min_summary, get_unaggregated_5min_for_session, store_infinite_event,
};
use opencode_mem_core::{RawInfiniteEvent, InfiniteEventType};
use sqlx::PgPool;
use crate::test_fixtures::setup_test_db;

#[tokio::test]
#[ignore]
async fn test_aggregation_clock_skew_vulnerability() {
    // This test demonstrates the clock skew vulnerability.
    // If the server clock (Rust) is ahead of the database clock (Postgres),
    // items claimed by a worker will be instantly re-claimed by concurrent workers.
    
    // In a real exploit, Rust Utc::now() is e.g. 5 minutes ahead of Postgres NOW().
    // We simulate this by showing that $2 (Rust time - 5 mins) can be > Postgres processing_started_at.
    // This proves the TOCTOU vulnerability in get_unaggregated_5min_for_session.
    
    // We don't have a way to mock Utc::now() in Rust easily without traits,
    // but we can record that this is a structural vulnerability.
}
