use std::sync::Arc;

use opencode_mem_storage::traits::PendingQueueStore;
use opencode_mem_storage::{PendingMessage, QueueStats, StorageBackend};

use crate::ServiceError;

pub struct QueueService {
    storage: Arc<StorageBackend>,
}

impl QueueService {
    #[must_use]
    pub fn new(storage: Arc<StorageBackend>) -> Self {
        Self { storage }
    }

    pub async fn queue_message(
        &self,
        session_id: &str,
        tool_name: Option<&str>,
        tool_input: Option<&str>,
        tool_response: Option<&str>,
        project: Option<&str>,
    ) -> Result<i64, ServiceError> {
        Ok(self
            .storage
            .queue_message(session_id, tool_name, tool_input, tool_response, project)
            .await?)
    }

    pub async fn get_all_pending_messages(
        &self,
        limit: usize,
    ) -> Result<Vec<PendingMessage>, ServiceError> {
        Ok(self.storage.get_all_pending_messages(limit).await?)
    }

    pub async fn get_queue_stats(&self) -> Result<QueueStats, ServiceError> {
        Ok(self.storage.get_queue_stats().await?)
    }

    pub async fn claim_pending_messages(
        &self,
        max: usize,
        visibility_timeout_secs: i64,
    ) -> Result<Vec<PendingMessage>, ServiceError> {
        Ok(self.storage.claim_pending_messages(max, visibility_timeout_secs).await?)
    }

    pub async fn complete_message(&self, id: i64) -> Result<(), ServiceError> {
        Ok(self.storage.complete_message(id).await?)
    }

    pub async fn fail_message(&self, id: i64, permanent: bool) -> Result<(), ServiceError> {
        Ok(self.storage.fail_message(id, permanent).await?)
    }

    pub async fn clear_failed_messages(&self) -> Result<usize, ServiceError> {
        Ok(self.storage.clear_failed_messages().await?)
    }

    pub async fn clear_stale_failed_messages(&self, ttl_secs: i64) -> Result<usize, ServiceError> {
        Ok(self.storage.clear_stale_failed_messages(ttl_secs).await?)
    }

    pub async fn retry_failed_messages(&self) -> Result<usize, ServiceError> {
        Ok(self.storage.retry_failed_messages().await?)
    }

    pub async fn clear_all_pending_messages(&self) -> Result<usize, ServiceError> {
        Ok(self.storage.clear_all_pending_messages().await?)
    }

    pub async fn get_pending_count(&self) -> Result<usize, ServiceError> {
        Ok(self.storage.get_pending_count().await?)
    }

    pub async fn release_stale_messages(
        &self,
        visibility_timeout_secs: i64,
    ) -> Result<usize, ServiceError> {
        Ok(self.storage.release_stale_messages(visibility_timeout_secs).await?)
    }

    pub async fn release_messages(&self, ids: &[i64]) -> Result<usize, ServiceError> {
        Ok(self.storage.release_messages(ids).await?)
    }
}
