//! Infinite AGI Memory - PostgreSQL + pgvector storage
//!
//! Stores all events (user, assistant, tool, decision, error, commit, delegation)
//! with hierarchical summarization using Gemini Flash.

mod backend;
mod compression;
mod event_queries;
mod event_types;
mod pipeline;
mod summary_queries;

pub use event_types::{
    assistant_event, tool_event, user_event, EventType, RawEvent, StoredEvent, Summary,
    SummaryEntities,
};
pub use opencode_mem_core::StorageBackend;

use anyhow::Result;
use chrono::{DateTime, Utc};
use opencode_mem_llm::LlmClient;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;

pub struct InfiniteMemory {
    pool: PgPool,
    llm: Arc<LlmClient>,
}

impl InfiniteMemory {
    pub async fn new(database_url: &str, llm: Arc<LlmClient>) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        Ok(Self { pool, llm })
    }

    pub async fn store_event(&self, event: RawEvent) -> Result<i64> {
        event_queries::store_event(&self.pool, event).await
    }

    pub async fn get_recent(&self, limit: i64) -> Result<Vec<StoredEvent>> {
        event_queries::get_recent(&self.pool, limit).await
    }

    pub async fn get_unsummarized_events(&self, limit: i64) -> Result<Vec<StoredEvent>> {
        event_queries::get_unsummarized_events(&self.pool, limit).await
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
        pipeline::create_5min_summary(&self.pool, events, summary, entities).await
    }

    pub async fn run_compression_pipeline(&self) -> Result<u32> {
        pipeline::run_compression_pipeline(&self.pool, &self.llm).await
    }

    pub async fn get_unaggregated_5min_summaries(&self, limit: i64) -> Result<Vec<Summary>> {
        summary_queries::get_unaggregated_5min_summaries(&self.pool, limit).await
    }

    pub async fn create_hour_summary(
        &self,
        summaries: &[Summary],
        content: &str,
        entities: Option<&SummaryEntities>,
    ) -> Result<i64> {
        pipeline::create_hour_summary(&self.pool, summaries, content, entities).await
    }

    pub async fn get_unaggregated_hour_summaries(&self, limit: i64) -> Result<Vec<Summary>> {
        summary_queries::get_unaggregated_hour_summaries(&self.pool, limit).await
    }

    pub async fn create_day_summary(
        &self,
        summaries: &[Summary],
        content: &str,
        entities: Option<&SummaryEntities>,
    ) -> Result<i64> {
        pipeline::create_day_summary(&self.pool, summaries, content, entities).await
    }

    pub async fn compress_summaries(&self, summaries: &[Summary]) -> Result<String> {
        compression::compress_summaries(&self.llm, summaries).await
    }

    pub async fn run_full_compression(&self) -> Result<(u32, u32, u32)> {
        pipeline::run_full_compression(&self.pool, &self.llm).await
    }

    pub async fn search(&self, query: &str, limit: i64) -> Result<Vec<StoredEvent>> {
        event_queries::search(&self.pool, query, limit).await
    }

    pub async fn stats(&self) -> Result<serde_json::Value> {
        event_queries::stats(&self.pool).await
    }

    pub async fn get_events_by_summary_id(
        &self,
        summary_5min_id: i64,
        limit: i64,
    ) -> Result<Vec<StoredEvent>> {
        event_queries::get_events_by_summary_id(&self.pool, summary_5min_id, limit).await
    }

    pub async fn get_events_by_time_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        session_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<StoredEvent>> {
        event_queries::get_events_by_time_range(&self.pool, start, end, session_id, limit).await
    }

    pub async fn get_summary_5min(&self, id: i64) -> Result<Option<Summary>> {
        summary_queries::get_summary_5min(&self.pool, id).await
    }

    pub async fn get_summary_hour(&self, id: i64) -> Result<Option<Summary>> {
        summary_queries::get_summary_hour(&self.pool, id).await
    }

    pub async fn get_summary_day(&self, id: i64) -> Result<Option<Summary>> {
        summary_queries::get_summary_day(&self.pool, id).await
    }

    pub async fn get_5min_summaries_by_hour_id(
        &self,
        hour_id: i64,
        limit: i64,
    ) -> Result<Vec<Summary>> {
        summary_queries::get_5min_summaries_by_hour_id(&self.pool, hour_id, limit).await
    }

    pub async fn search_by_entity(
        &self,
        entity_type: &str,
        value: &str,
        limit: i64,
    ) -> Result<Vec<Summary>> {
        summary_queries::search_by_entity(&self.pool, entity_type, value, limit).await
    }

    pub async fn get_hour_summaries_by_day_id(
        &self,
        day_id: i64,
        limit: i64,
    ) -> Result<Vec<Summary>> {
        summary_queries::get_hour_summaries_by_day_id(&self.pool, day_id, limit).await
    }
}
