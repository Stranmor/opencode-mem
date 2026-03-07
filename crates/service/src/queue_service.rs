use std::sync::Arc;

use opencode_mem_core::{ProjectFilter, ToolCall, cap_query_limit, sanitize_input};
use opencode_mem_storage::traits::PendingQueueStore;
use opencode_mem_storage::{PendingMessage, QueueStats, StorageBackend};

use crate::{PendingWriteQueue, ServiceError};

/// Result of attempting to queue a tool call.
pub enum QueueToolCallResult {
    /// Queued successfully, contains the message ID.
    Queued(i64),
    /// Skipped because the project is excluded by `ProjectFilter`.
    ExcludedProject,
}

pub struct QueueService {
    storage: Arc<StorageBackend>,
    pending_writes: Arc<PendingWriteQueue>,
}

impl QueueService {
    #[must_use]
    pub fn new(storage: Arc<StorageBackend>, pending_writes: Arc<PendingWriteQueue>) -> Self {
        Self {
            storage,
            pending_writes,
        }
    }

    /// Push a write operation to the in-memory buffer for later flush.
    pub fn push_pending_write(&self, write: crate::PendingWrite) {
        self.pending_writes.push(write);
    }

    /// Queue a single tool call with sanitization and project exclusion.
    ///
    /// Applies `ProjectFilter` check and `sanitize_input` on tool input/output
    /// before inserting into the pending queue. Returns `ExcludedProject` if the
    /// tool call's project is excluded, so callers can skip without error.
    pub async fn queue_tool_call(
        &self,
        tool_call: &ToolCall,
    ) -> Result<QueueToolCallResult, ServiceError> {
        if let Some(project) = tool_call.project.as_deref() {
            if ProjectFilter::global().is_some_and(|filter| filter.is_excluded(project)) {
                return Ok(QueueToolCallResult::ExcludedProject);
            }
        }

        // Use recursive JSON sanitization to avoid corrupting JSON envelopes (SPOT compliance with Infinite Memory path)
        let mut sanitized_input = tool_call.input.clone();
        opencode_mem_core::sanitize_json_values(&mut sanitized_input);
        let tool_input_str = serde_json::to_string(&sanitized_input).ok();

        let filtered_output = sanitize_input(&tool_call.output);

        let id = self
            .storage
            .queue_message(
                &tool_call.session_id,
                Some(&tool_call.call_id),
                Some(&tool_call.tool),
                tool_input_str.as_deref(),
                Some(&filtered_output),
                tool_call.project.as_deref(),
            )
            .await?;

        Ok(QueueToolCallResult::Queued(id))
    }

    /// Queue multiple tool calls, returning the number successfully queued.
    ///
    /// Excluded projects are silently skipped (not counted as errors).
    pub async fn queue_tool_calls(&self, tool_calls: &[ToolCall]) -> Result<usize, ServiceError> {
        let mut count = 0usize;
        for tool_call in tool_calls {
            match self.queue_tool_call(tool_call).await? {
                QueueToolCallResult::Queued(_) => count = count.saturating_add(1),
                QueueToolCallResult::ExcludedProject => {}
            }
        }
        Ok(count)
    }

    /// Check if a project is excluded by the global `ProjectFilter`.
    #[must_use]
    pub fn is_project_excluded(project: Option<&str>) -> bool {
        if let Some(project) = project {
            if ProjectFilter::global().is_some_and(|filter| filter.is_excluded(project)) {
                return true;
            }
        }
        false
    }

    #[must_use]
    pub fn should_skip_project(project: Option<&str>) -> bool {
        if let Some(value) = project {
            if value.is_empty() || value == "unknown" {
                return true;
            }
        }
        Self::is_project_excluded(project)
    }

    pub async fn queue_message(
        &self,
        session_id: &str,
        call_id: Option<&str>,
        tool_name: Option<&str>,
        tool_input: Option<&str>,
        tool_response: Option<&str>,
        project: Option<&str>,
    ) -> Result<i64, ServiceError> {
        Ok(self
            .storage
            .queue_message(
                session_id,
                call_id,
                tool_name,
                tool_input,
                tool_response,
                project,
            )
            .await?)
    }

    pub async fn get_all_pending_messages(
        &self,
        limit: usize,
    ) -> Result<Vec<PendingMessage>, ServiceError> {
        let limit = cap_query_limit(limit);
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
        Ok(self
            .storage
            .claim_pending_messages(max, visibility_timeout_secs)
            .await?)
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
        Ok(self
            .storage
            .release_stale_messages(visibility_timeout_secs)
            .await?)
    }

    pub async fn release_messages(&self, ids: &[i64]) -> Result<usize, ServiceError> {
        Ok(self.storage.release_messages(ids).await?)
    }
}
