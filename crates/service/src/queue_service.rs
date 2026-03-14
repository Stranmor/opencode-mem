use std::sync::Arc;

use opencode_mem_core::{ToolCall, cap_query_limit, sanitize_input};
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
    project_filter: Option<opencode_mem_core::ProjectFilter>,
}

impl QueueService {
    #[must_use]
    pub fn new(
        storage: Arc<StorageBackend>,
        pending_writes: Arc<PendingWriteQueue>,
        config: &opencode_mem_core::AppConfig,
    ) -> Self {
        Self {
            storage,
            pending_writes,
            project_filter: opencode_mem_core::ProjectFilter::new(
                config.excluded_projects_raw.as_deref(),
            ),
        }
    }

    pub fn circuit_breaker(&self) -> &opencode_mem_storage::CircuitBreaker {
        self.storage.circuit_breaker()
    }

    pub(crate) fn with_cb<T>(&self, result: Result<T, ServiceError>) -> Result<T, ServiceError> {
        result
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
        if let Some(project) = tool_call.project.as_deref()
            && self.is_project_excluded(Some(project))
        {
            return Ok(QueueToolCallResult::ExcludedProject);
        }

        // Use recursive JSON sanitization to avoid corrupting JSON envelopes (SPOT compliance with Infinite Memory path)
        let mut sanitized_input = tool_call.input.clone();
        opencode_mem_core::sanitize_json_values(&mut sanitized_input);
        let tool_input_str = serde_json::to_string(&sanitized_input).ok();

        let filtered_output = sanitize_input(&tool_call.output);

        let result = self
            .storage
            .guarded(|| {
                self.storage.queue_message(
                    &tool_call.session_id,
                    Some(&tool_call.call_id),
                    Some(&tool_call.tool),
                    tool_input_str.as_deref(),
                    Some(&filtered_output),
                    tool_call.project.as_deref(),
                )
            })
            .await;
        let id = self.with_cb(result.map_err(ServiceError::from))?;

        Ok(QueueToolCallResult::Queued(id))
    }

    /// Queue multiple tool calls, returning the number successfully queued.
    ///
    /// Excluded projects are silently skipped (not counted as errors).
    pub async fn queue_tool_calls(&self, tool_calls: &[ToolCall]) -> Result<usize, ServiceError> {
        if tool_calls.is_empty() {
            return Ok(0);
        }

        let mut messages = Vec::with_capacity(tool_calls.len());

        for tool_call in tool_calls {
            if self.is_project_excluded(tool_call.project.as_deref()) {
                continue;
            }

            let mut sanitized_input = tool_call.input.clone();
            opencode_mem_core::sanitize_json_values(&mut sanitized_input);
            let tool_input_str = serde_json::to_string(&sanitized_input).ok();
            let filtered_output = sanitize_input(&tool_call.output);

            messages.push(PendingMessage::new(
                tool_call.session_id.to_string(),
                Some(tool_call.call_id.clone()),
                Some(tool_call.tool.clone()),
                tool_input_str,
                Some(filtered_output),
                tool_call.project.clone(),
            ));
        }

        if messages.is_empty() {
            return Ok(0);
        }

        let result = self
            .storage
            .guarded(|| self.storage.queue_messages(&messages))
            .await;
        self.with_cb(result.map_err(ServiceError::from))
    }

    /// Check if a project is excluded by the current `ProjectFilter`.
    ///
    /// Normalizes the project name via `ProjectId` before checking,
    /// so that `My-Secret/` is correctly matched against a pattern for `my_secret`.
    #[must_use]
    pub fn is_project_excluded(&self, project: Option<&str>) -> bool {
        if let Some(project) = project
            && let Some(ref filter) = self.project_filter
        {
            let normalized = opencode_mem_core::ProjectId::new(project).to_string();
            return filter.is_excluded(&normalized);
        }
        false
    }

    #[must_use]
    pub fn should_skip_project(&self, project: Option<&str>) -> bool {
        if let Some(value) = project
            && (value.is_empty() || value == "unknown")
        {
            return true;
        }
        self.is_project_excluded(project)
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
        let result = self
            .storage
            .guarded(|| {
                self.storage.queue_message(
                    session_id,
                    call_id,
                    tool_name,
                    tool_input,
                    tool_response,
                    project,
                )
            })
            .await;
        self.with_cb(result.map_err(ServiceError::from))
    }

    pub async fn get_all_pending_messages(
        &self,
        limit: usize,
    ) -> Result<Vec<PendingMessage>, ServiceError> {
        let limit = cap_query_limit(limit);
        let result = self
            .storage
            .guarded(|| self.storage.get_all_pending_messages(limit))
            .await;
        self.with_cb(result.map_err(ServiceError::from))
    }

    pub async fn get_queue_stats(&self) -> Result<QueueStats, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.get_queue_stats())
            .await;
        self.with_cb(result.map_err(ServiceError::from))
    }

    pub async fn claim_pending_messages(
        &self,
        max: usize,
        visibility_timeout_secs: i64,
    ) -> Result<Vec<PendingMessage>, ServiceError> {
        let result = self
            .storage
            .guarded(|| {
                self.storage
                    .claim_pending_messages(max, visibility_timeout_secs)
            })
            .await;
        self.with_cb(result.map_err(ServiceError::from))
    }

    pub async fn complete_message(&self, id: i64) -> Result<(), ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.complete_message(id))
            .await;
        self.with_cb(result.map_err(ServiceError::from))
    }

    pub async fn fail_message(&self, id: i64, permanent: bool) -> Result<(), ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.fail_message(id, permanent))
            .await;
        self.with_cb(result.map_err(ServiceError::from))
    }

    pub async fn clear_failed_messages(&self) -> Result<usize, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.clear_failed_messages())
            .await;
        self.with_cb(result.map_err(ServiceError::from))
    }

    pub async fn clear_stale_failed_messages(&self, ttl_secs: i64) -> Result<usize, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.clear_stale_failed_messages(ttl_secs))
            .await;
        self.with_cb(result.map_err(ServiceError::from))
    }

    pub async fn retry_failed_messages(&self) -> Result<usize, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.retry_failed_messages())
            .await;
        self.with_cb(result.map_err(ServiceError::from))
    }

    pub async fn clear_all_pending_messages(&self) -> Result<usize, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.clear_all_pending_messages())
            .await;
        self.with_cb(result.map_err(ServiceError::from))
    }

    pub async fn get_pending_count(&self) -> Result<usize, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.get_pending_count())
            .await;
        self.with_cb(result.map_err(ServiceError::from))
    }

    pub async fn release_stale_messages(
        &self,
        visibility_timeout_secs: i64,
    ) -> Result<usize, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.release_stale_messages(visibility_timeout_secs))
            .await;
        self.with_cb(result.map_err(ServiceError::from))
    }

    pub async fn release_messages(&self, ids: &[i64]) -> Result<usize, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.release_messages(ids))
            .await;
        self.with_cb(result.map_err(ServiceError::from))
    }
}
