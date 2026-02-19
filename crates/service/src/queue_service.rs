use std::sync::Arc;

use opencode_mem_storage::traits::PendingQueueStore;
use opencode_mem_storage::{PendingMessage, QueueStats, StorageBackend};

/// Service layer for pending message queue operations.
///
/// Wraps `PendingQueueStore` trait calls, providing a single entry point
/// for HTTP handlers. Enables future cross-cutting concerns (logging,
/// metrics, rate limiting) in one place.
pub struct QueueService {
    storage: Arc<StorageBackend>,
}

impl QueueService {
    #[must_use]
    pub fn new(storage: Arc<StorageBackend>) -> Self {
        Self { storage }
    }

    /// Queue a message for processing. Returns the new message ID.
    pub async fn queue_message(
        &self,
        session_id: &str,
        tool_name: Option<&str>,
        tool_input: Option<&str>,
        tool_response: Option<&str>,
        project: Option<&str>,
    ) -> anyhow::Result<i64> {
        self.storage.queue_message(session_id, tool_name, tool_input, tool_response, project).await.map_err(Into::into)
    }

    /// Get all pending messages up to `limit`.
    pub async fn get_all_pending_messages(
        &self,
        limit: usize,
    ) -> anyhow::Result<Vec<PendingMessage>> {
        self.storage.get_all_pending_messages(limit).await.map_err(Into::into)
    }

    /// Get queue statistics (pending, processing, failed counts).
    pub async fn get_queue_stats(&self) -> anyhow::Result<QueueStats> {
        self.storage.get_queue_stats().await.map_err(Into::into)
    }

    /// Claim pending messages for processing with visibility timeout.
    pub async fn claim_pending_messages(
        &self,
        max: usize,
        visibility_timeout_secs: i64,
    ) -> anyhow::Result<Vec<PendingMessage>> {
        self.storage.claim_pending_messages(max, visibility_timeout_secs).await.map_err(Into::into)
    }

    /// Mark message as successfully processed.
    pub async fn complete_message(&self, id: i64) -> anyhow::Result<()> {
        self.storage.complete_message(id).await.map_err(Into::into)
    }

    /// Mark message as failed. If `permanent` is true, increments retry count.
    pub async fn fail_message(&self, id: i64, permanent: bool) -> anyhow::Result<()> {
        self.storage.fail_message(id, permanent).await.map_err(Into::into)
    }

    /// Clear all failed messages from the queue.
    pub async fn clear_failed_messages(&self) -> anyhow::Result<usize> {
        self.storage.clear_failed_messages().await.map_err(Into::into)
    }

    /// Reset failed messages back to pending for retry.
    pub async fn retry_failed_messages(&self) -> anyhow::Result<usize> {
        self.storage.retry_failed_messages().await.map_err(Into::into)
    }

    /// Clear all pending messages from the queue.
    pub async fn clear_all_pending_messages(&self) -> anyhow::Result<usize> {
        self.storage.clear_all_pending_messages().await.map_err(Into::into)
    }

    /// Get count of pending messages.
    pub async fn get_pending_count(&self) -> anyhow::Result<usize> {
        self.storage.get_pending_count().await.map_err(Into::into)
    }

    /// Release stale processing messages back to pending.
    pub async fn release_stale_messages(
        &self,
        visibility_timeout_secs: i64,
    ) -> anyhow::Result<usize> {
        self.storage.release_stale_messages(visibility_timeout_secs).await.map_err(Into::into)
    }
}
