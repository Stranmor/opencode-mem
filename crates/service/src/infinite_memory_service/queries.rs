use opencode_mem_core::{InfiniteSummary, StoredInfiniteEvent};
use opencode_mem_storage::StorageError;

use chrono::{DateTime, Utc};

use super::InfiniteMemoryService;

impl InfiniteMemoryService {
    pub async fn get_recent(&self, limit: i64) -> Result<Vec<StoredInfiniteEvent>, StorageError> {
        let result = opencode_mem_storage::pg_storage::infinite_memory::get_recent_infinite_events(
            &self.pool, limit,
        )
        .await;
        self.record_result_storage(&result);
        result
    }

    pub async fn get_unsummarized_events(
        &self,
        limit: i64,
    ) -> Result<Vec<StoredInfiniteEvent>, StorageError> {
        let result =
            opencode_mem_storage::pg_storage::infinite_memory::get_unsummarized_infinite_events(
                &self.pool, limit,
            )
            .await;
        self.record_result_storage(&result);
        result
    }

    pub async fn get_unaggregated_5min_summaries(
        &self,
        limit: i64,
    ) -> Result<Vec<InfiniteSummary>, StorageError> {
        let result =
            opencode_mem_storage::pg_storage::infinite_memory::get_unaggregated_5min_summaries(
                &self.pool, limit,
            )
            .await;
        self.record_result_storage(&result);
        result
    }

    pub async fn get_unaggregated_hour_summaries(
        &self,
        limit: i64,
    ) -> Result<Vec<InfiniteSummary>, StorageError> {
        let result =
            opencode_mem_storage::pg_storage::infinite_memory::get_unaggregated_hour_summaries(
                &self.pool, limit,
            )
            .await;
        self.record_result_storage(&result);
        result
    }

    pub async fn search(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<StoredInfiniteEvent>, StorageError> {
        let result = opencode_mem_storage::pg_storage::infinite_memory::search_infinite_events(
            &self.pool, query, limit,
        )
        .await;
        self.record_result_storage(&result);
        result
    }

    pub async fn stats(&self) -> Result<serde_json::Value, StorageError> {
        let result =
            opencode_mem_storage::pg_storage::infinite_memory::infinite_memory_stats(&self.pool)
                .await;
        self.record_result_storage(&result);
        result
    }

    pub async fn get_events_by_summary_id(
        &self,
        summary_5min_id: i64,
        limit: i64,
    ) -> Result<Vec<StoredInfiniteEvent>, StorageError> {
        let result =
            opencode_mem_storage::pg_storage::infinite_memory::get_infinite_events_by_summary_id(
                &self.pool,
                summary_5min_id,
                limit,
            )
            .await;
        self.record_result_storage(&result);
        result
    }

    pub async fn get_events_by_time_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        session_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<StoredInfiniteEvent>, StorageError> {
        let result =
            opencode_mem_storage::pg_storage::infinite_memory::get_infinite_events_by_time_range(
                &self.pool, start, end, session_id, limit,
            )
            .await;
        self.record_result_storage(&result);
        result
    }

    pub async fn get_summary_5min(&self, id: i64) -> Result<Option<InfiniteSummary>, StorageError> {
        let result = opencode_mem_storage::pg_storage::infinite_memory::get_infinite_summary_5min(
            &self.pool, id,
        )
        .await;
        self.record_result_storage(&result);
        result
    }

    pub async fn get_summary_hour(&self, id: i64) -> Result<Option<InfiniteSummary>, StorageError> {
        let result = opencode_mem_storage::pg_storage::infinite_memory::get_infinite_summary_hour(
            &self.pool, id,
        )
        .await;
        self.record_result_storage(&result);
        result
    }

    pub async fn get_summary_day(&self, id: i64) -> Result<Option<InfiniteSummary>, StorageError> {
        let result = opencode_mem_storage::pg_storage::infinite_memory::get_infinite_summary_day(
            &self.pool, id,
        )
        .await;
        self.record_result_storage(&result);
        result
    }

    pub async fn get_5min_summaries_by_hour_id(
        &self,
        hour_id: i64,
        limit: i64,
    ) -> Result<Vec<InfiniteSummary>, StorageError> {
        let result =
            opencode_mem_storage::pg_storage::infinite_memory::get_5min_summaries_by_hour_id(
                &self.pool, hour_id, limit,
            )
            .await;
        self.record_result_storage(&result);
        result
    }

    pub async fn search_by_entity(
        &self,
        entity_type: &str,
        value: &str,
        limit: i64,
    ) -> Result<Vec<InfiniteSummary>, StorageError> {
        let result = opencode_mem_storage::pg_storage::infinite_memory::search_by_entity(
            &self.pool,
            entity_type,
            value,
            limit,
        )
        .await;
        self.record_result_storage(&result);
        result
    }

    pub async fn get_hour_summaries_by_day_id(
        &self,
        day_id: i64,
        limit: i64,
    ) -> Result<Vec<InfiniteSummary>, StorageError> {
        let result =
            opencode_mem_storage::pg_storage::infinite_memory::get_hour_summaries_by_day_id(
                &self.pool, day_id, limit,
            )
            .await;
        self.record_result_storage(&result);
        result
    }
}
