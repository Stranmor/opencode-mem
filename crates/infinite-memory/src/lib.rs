//! Infinite AGI Memory - `PostgreSQL` + pgvector storage
//!
//! Stores all events (user, assistant, tool, decision, error, commit, delegation)
//! with hierarchical summarization using Gemini Flash.

#![allow(missing_docs, reason = "Internal crate with self-explanatory API")]
#![allow(
    unreachable_pub,
    reason = "pub items in private modules are re-exported via pub use in lib.rs"
)]
#![allow(
    unused_results,
    reason = "SQL execute() returns row count which is often unused"
)]
#![allow(
    clippy::indexing_slicing,
    reason = "SQL query results have known structure"
)]
#![allow(
    clippy::arithmetic_side_effects,
    reason = "Timestamp arithmetic is safe within reasonable bounds"
)]
#![allow(
    clippy::as_conversions,
    reason = "i64 to u64 conversions for timestamps are safe"
)]
#![allow(
    clippy::needless_raw_strings,
    reason = "SQL strings use raw for readability"
)]
#![allow(clippy::str_to_string, reason = "to_string on &str is idiomatic")]
#![allow(clippy::expect_used, reason = "Expects are used for known-valid data")]
#![allow(clippy::absolute_paths, reason = "Explicit paths for clarity")]
#![allow(
    clippy::cast_possible_wrap,
    reason = "usize to i64 is safe for reasonable sizes"
)]
#![allow(
    clippy::non_ascii_literal,
    reason = "Unicode in strings is intentional"
)]
#![allow(missing_debug_implementations, reason = "Internal types")]
#![allow(missing_copy_implementations, reason = "Types may grow")]
#![allow(clippy::cast_possible_truncation, reason = "Sizes are within bounds")]
#![allow(
    clippy::missing_docs_in_private_items,
    reason = "Internal crate modules"
)]
#![allow(clippy::implicit_return, reason = "Implicit return is idiomatic Rust")]
#![allow(clippy::question_mark_used, reason = "? operator is idiomatic Rust")]
#![allow(clippy::min_ident_chars, reason = "Short closure params are idiomatic")]
#![allow(
    clippy::shadow_reuse,
    reason = "Shadowing for transformations is idiomatic"
)]
#![allow(clippy::exhaustive_enums, reason = "Internal event types are stable")]
#![allow(clippy::exhaustive_structs, reason = "Internal event types are stable")]
#![allow(
    clippy::single_call_fn,
    reason = "Helper functions improve readability"
)]

mod compression;
mod event_queries;
mod event_types;
mod migrations;
mod pipeline;
mod summary_queries;

pub use event_types::{
    EventType, RawEvent, StoredEvent, Summary, SummaryEntities, assistant_event, tool_event,
    user_event,
};

use anyhow::Result;
use chrono::{DateTime, Utc};

use opencode_mem_llm::LlmClient;
use opencode_mem_storage::CircuitBreaker;

use sqlx::PgPool;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct InfiniteMemory {
    pool: PgPool,
    llm: Arc<LlmClient>,
    circuit_breaker: Arc<CircuitBreaker>,
    migrations_pending: Arc<AtomicBool>,
}

impl InfiniteMemory {
    pub async fn new(pool: sqlx::PgPool, llm: Arc<LlmClient>) -> Result<Self> {
        // Try migrations — if DB is unavailable (lazy pool), log warning and continue
        let migrations_pending = match migrations::run_migrations(&pool).await {
            Ok(()) => {
                tracing::info!("Infinite Memory migrations completed");
                false
            }
            Err(e) => {
                tracing::warn!(
                    "Infinite Memory started without migrations (DB may be unavailable): {e}"
                );
                true
            }
        };

        Ok(Self {
            pool,
            llm,
            circuit_breaker: Arc::new(CircuitBreaker::new()),
            migrations_pending: Arc::new(AtomicBool::new(migrations_pending)),
        })
    }

    /// Access the circuit breaker for health monitoring.
    #[must_use]
    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }

    /// Attempt to run pending migrations. Safe to call repeatedly — idempotent.
    pub async fn try_run_migrations(&self) -> bool {
        if self
            .migrations_pending
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }

        match migrations::run_migrations(&self.pool).await {
            Ok(()) => {
                tracing::info!("Infinite Memory deferred migrations completed successfully");
                true
            }
            Err(e) => {
                self.migrations_pending.store(true, Ordering::Release);
                tracing::warn!("Infinite Memory deferred migration attempt failed: {e}");
                false
            }
        }
    }

    #[must_use]
    pub fn has_pending_migrations(&self) -> bool {
        self.migrations_pending.load(Ordering::Acquire)
    }

    fn record_result<T>(&self, result: &Result<T>) {
        if result.is_ok() {
            let recovered = self.circuit_breaker.record_success();
            if recovered && self.migrations_pending.load(Ordering::Acquire) {
                let this = self.clone();
                tokio::spawn(async move {
                    let _ = this.try_run_migrations().await;
                });
            }
        } else if let Err(e) = result {
            let msg = e.to_string();
            // Do not trip circuit breaker on validation or logical errors
            if msg.contains("Invalid entity_type") {
                tracing::debug!("Validation error, ignoring for circuit breaker: {}", msg);
            } else {
                self.circuit_breaker.record_failure();
            }
        }
    }

    pub async fn store_event(&self, event: RawEvent) -> Result<i64> {
        let result = event_queries::store_event(&self.pool, event).await;
        self.record_result(&result);
        result
    }

    pub async fn get_recent(&self, limit: i64) -> Result<Vec<StoredEvent>> {
        let result = event_queries::get_recent(&self.pool, limit).await;
        self.record_result(&result);
        result
    }

    pub async fn get_unsummarized_events(&self, limit: i64) -> Result<Vec<StoredEvent>> {
        let result = event_queries::get_unsummarized_events(&self.pool, limit).await;
        self.record_result(&result);
        result
    }

    pub async fn compress_events(
        &self,
        events: &[StoredEvent],
    ) -> Result<(String, Option<SummaryEntities>)> {
        compression::compress_events(&self.llm, events).await
    }

    pub async fn create_5min_summary(
        &self,
        events: &[StoredEvent],
        summary: &str,
        entities: Option<&SummaryEntities>,
    ) -> Result<i64> {
        let result = pipeline::create_5min_summary(&self.pool, events, summary, entities).await;
        self.record_result(&result);
        result
    }

    pub async fn run_compression_pipeline(&self) -> Result<u32> {
        pipeline::run_compression_pipeline(&self.pool, &self.llm).await
    }

    pub async fn get_unaggregated_5min_summaries(&self, limit: i64) -> Result<Vec<Summary>> {
        let result = summary_queries::get_unaggregated_5min_summaries(&self.pool, limit).await;
        self.record_result(&result);
        result
    }

    pub async fn create_hour_summary(
        &self,
        summaries: &[Summary],
        content: &str,
        entities: Option<&SummaryEntities>,
    ) -> Result<i64> {
        let result = pipeline::create_hour_summary(&self.pool, summaries, content, entities).await;
        self.record_result(&result);
        result
    }

    pub async fn get_unaggregated_hour_summaries(&self, limit: i64) -> Result<Vec<Summary>> {
        let result = summary_queries::get_unaggregated_hour_summaries(&self.pool, limit).await;
        self.record_result(&result);
        result
    }

    pub async fn create_day_summary(
        &self,
        summaries: &[Summary],
        content: &str,
        entities: Option<&SummaryEntities>,
    ) -> Result<i64> {
        let result = pipeline::create_day_summary(&self.pool, summaries, content, entities).await;
        self.record_result(&result);
        result
    }

    pub async fn compress_summaries(&self, summaries: &[Summary]) -> Result<String> {
        compression::compress_summaries(&self.llm, summaries).await
    }

    pub async fn run_full_compression(&self) -> Result<(u32, u32, u32)> {
        pipeline::run_full_compression(&self.pool, &self.llm).await
    }

    pub async fn search(&self, query: &str, limit: i64) -> Result<Vec<StoredEvent>> {
        let result = event_queries::search(&self.pool, query, limit).await;
        self.record_result(&result);
        result
    }

    pub async fn stats(&self) -> Result<serde_json::Value> {
        let result = event_queries::stats(&self.pool).await;
        self.record_result(&result);
        result
    }

    pub async fn get_events_by_summary_id(
        &self,
        summary_5min_id: i64,
        limit: i64,
    ) -> Result<Vec<StoredEvent>> {
        let result =
            event_queries::get_events_by_summary_id(&self.pool, summary_5min_id, limit).await;
        self.record_result(&result);
        result
    }

    pub async fn get_events_by_time_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        session_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<StoredEvent>> {
        let result =
            event_queries::get_events_by_time_range(&self.pool, start, end, session_id, limit)
                .await;
        self.record_result(&result);
        result
    }

    pub async fn get_summary_5min(&self, id: i64) -> Result<Option<Summary>> {
        let result = summary_queries::get_summary_5min(&self.pool, id).await;
        self.record_result(&result);
        result
    }

    pub async fn get_summary_hour(&self, id: i64) -> Result<Option<Summary>> {
        let result = summary_queries::get_summary_hour(&self.pool, id).await;
        self.record_result(&result);
        result
    }

    pub async fn get_summary_day(&self, id: i64) -> Result<Option<Summary>> {
        let result = summary_queries::get_summary_day(&self.pool, id).await;
        self.record_result(&result);
        result
    }

    pub async fn get_5min_summaries_by_hour_id(
        &self,
        hour_id: i64,
        limit: i64,
    ) -> Result<Vec<Summary>> {
        let result =
            summary_queries::get_5min_summaries_by_hour_id(&self.pool, hour_id, limit).await;
        self.record_result(&result);
        result
    }

    pub async fn search_by_entity(
        &self,
        entity_type: &str,
        value: &str,
        limit: i64,
    ) -> Result<Vec<Summary>> {
        let result = summary_queries::search_by_entity(&self.pool, entity_type, value, limit).await;
        self.record_result(&result);
        result
    }

    pub async fn get_hour_summaries_by_day_id(
        &self,
        day_id: i64,
        limit: i64,
    ) -> Result<Vec<Summary>> {
        let result = summary_queries::get_hour_summaries_by_day_id(&self.pool, day_id, limit).await;
        self.record_result(&result);
        result
    }
}

impl Clone for InfiniteMemory {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            llm: Arc::clone(&self.llm),
            circuit_breaker: Arc::clone(&self.circuit_breaker),
            migrations_pending: Arc::clone(&self.migrations_pending),
        }
    }
}
