//! Integration tests for PgStorage.
//! Run with: DATABASE_URL=... cargo test -p opencode-mem-storage -- --ignored pg_

#![allow(clippy::unwrap_used, reason = "integration test code")]

mod embedding_tests;
mod helpers;
mod knowledge_tests;
mod observation_tests;
mod queue_tests;
mod search_tests;
mod session_tests;
mod stats_tests;
