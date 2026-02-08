//! Storage types shared across modules

use serde::{Deserialize, Serialize};
use std::env;
use std::str::FromStr;

/// Statistics about storage contents
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[non_exhaustive]
pub struct StorageStats {
    /// Number of observations in storage.
    pub observation_count: u64,
    /// Number of sessions in storage.
    pub session_count: u64,
    /// Number of session summaries in storage.
    pub summary_count: u64,
    /// Number of user prompts in storage.
    pub prompt_count: u64,
    /// Number of projects in storage.
    pub project_count: u64,
}

/// Generic paginated result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PaginatedResult<T> {
    /// Items in the current page.
    pub items: Vec<T>,
    /// Total number of items across all pages.
    pub total: u64,
    /// Offset from the start.
    pub offset: u64,
    /// Maximum items per page.
    pub limit: u64,
}

/// Status of a pending message in the processing queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PendingMessageStatus {
    /// Message is waiting to be processed.
    Pending,
    /// Message is currently being processed.
    Processing,
    /// Message has been successfully processed.
    Processed,
    /// Message processing failed.
    Failed,
}

impl PendingMessageStatus {
    /// Returns the string representation of the status.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match *self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Processed => "processed",
            Self::Failed => "failed",
        }
    }
}

impl FromStr for PendingMessageStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "processing" => Ok(Self::Processing),
            "processed" => Ok(Self::Processed),
            "failed" => Ok(Self::Failed),
            _ => Err(anyhow::anyhow!("Invalid pending message status: {s}")),
        }
    }
}

/// A message in the pending processing queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PendingMessage {
    /// Unique database ID.
    pub id: i64,
    /// Session this message belongs to.
    pub session_id: String,
    /// Current processing status.
    pub status: PendingMessageStatus,
    /// Name of the tool that was called.
    pub tool_name: Option<String>,
    /// Input provided to the tool.
    pub tool_input: Option<String>,
    /// Response from the tool.
    pub tool_response: Option<String>,
    /// Number of retry attempts.
    pub retry_count: i32,
    /// Unix timestamp when message was created.
    pub created_at_epoch: i64,
    /// Unix timestamp when message was claimed for processing.
    pub claimed_at_epoch: Option<i64>,
    /// Unix timestamp when processing completed.
    pub completed_at_epoch: Option<i64>,
    /// Project this message belongs to.
    pub project: Option<String>,
}

/// Returns the maximum retry count from environment or default (3).
#[must_use]
pub fn max_retry_count() -> i32 {
    env::var("OPENCODE_MEM_MAX_RETRY").ok().and_then(|v| v.parse().ok()).unwrap_or(3i32)
}

/// Returns the default visibility timeout in seconds from environment or default (300).
#[must_use]
pub fn default_visibility_timeout_secs() -> i64 {
    env::var("OPENCODE_MEM_VISIBILITY_TIMEOUT").ok().and_then(|v| v.parse().ok()).unwrap_or(300i64)
}

/// Statistics about the pending message queue.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[non_exhaustive]
pub struct QueueStats {
    /// Number of messages waiting to be processed.
    pub pending: u64,
    /// Number of messages currently being processed.
    pub processing: u64,
    /// Number of messages that failed processing.
    pub failed: u64,
    /// Number of successfully processed messages.
    pub processed: u64,
}
