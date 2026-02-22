use async_trait::async_trait;

use crate::error::StorageError;
use crate::pending_queue::{PendingMessage, QueueStats};

/// Pending message queue operations.
#[async_trait]
pub trait PendingQueueStore: Send + Sync {
    /// Queue a message for processing. Returns the new message ID.
    async fn queue_message(
        &self,
        session_id: &str,
        tool_name: Option<&str>,
        tool_input: Option<&str>,
        tool_response: Option<&str>,
        project: Option<&str>,
    ) -> Result<i64, StorageError>;

    /// Claim pending messages for processing.
    async fn claim_pending_messages(
        &self,
        limit: usize,
        visibility_timeout_secs: i64,
    ) -> Result<Vec<PendingMessage>, StorageError>;

    /// Delete message after successful processing.
    async fn complete_message(&self, id: i64) -> Result<(), StorageError>;

    /// Mark message as failed.
    async fn fail_message(&self, id: i64, increment_retry: bool) -> Result<(), StorageError>;

    /// Get count of pending messages.
    async fn get_pending_count(&self) -> Result<usize, StorageError>;

    /// Release stale processing messages back to pending.
    async fn release_stale_messages(
        &self,
        visibility_timeout_secs: i64,
    ) -> Result<usize, StorageError>;

    /// Release specific messages back to pending immediately.
    async fn release_messages(&self, ids: &[i64]) -> Result<usize, StorageError>;

    /// Get failed messages.
    async fn get_failed_messages(&self, limit: usize) -> Result<Vec<PendingMessage>, StorageError>;

    /// Get all pending messages.
    async fn get_all_pending_messages(
        &self,
        limit: usize,
    ) -> Result<Vec<PendingMessage>, StorageError>;

    /// Get queue statistics.
    async fn get_queue_stats(&self) -> Result<QueueStats, StorageError>;

    /// Clear all failed messages.
    async fn clear_failed_messages(&self) -> Result<usize, StorageError>;

    /// Delete failed messages older than TTL (dead letter queue garbage collection).
    async fn clear_stale_failed_messages(&self, ttl_secs: i64) -> Result<usize, StorageError>;

    /// Reset failed messages back to pending for retry.
    async fn retry_failed_messages(&self) -> Result<usize, StorageError>;

    /// Clear all pending messages.
    async fn clear_all_pending_messages(&self) -> Result<usize, StorageError>;
}
