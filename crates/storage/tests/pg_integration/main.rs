//! Integration tests for PgStorage.
//! Run with: DATABASE_URL=... cargo test -p opencode-mem-storage -- --ignored pg_

#![allow(clippy::unwrap_used, reason = "integration test code")]

mod helpers;
mod observation_tests;
mod session_tests;
mod knowledge_tests;
mod queue_tests;
mod search_tests;
mod stats_tests;
mod embedding_tests;
