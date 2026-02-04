//! Storage types shared across modules

use serde::{Deserialize, Serialize};

/// Statistics about storage contents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub observation_count: u64,
    pub session_count: u64,
    pub summary_count: u64,
    pub prompt_count: u64,
    pub project_count: u64,
}

/// Generic paginated result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PendingMessageStatus {
    Pending,
    Processing,
    Processed,
    Failed,
}

impl PendingMessageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Processed => "processed",
            Self::Failed => "failed",
        }
    }
}

impl std::str::FromStr for PendingMessageStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "processing" => Ok(Self::Processing),
            "processed" => Ok(Self::Processed),
            "failed" => Ok(Self::Failed),
            _ => Err(anyhow::anyhow!("Invalid pending message status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMessage {
    pub id: i64,
    pub session_id: String,
    pub status: PendingMessageStatus,
    pub tool_name: Option<String>,
    pub tool_input: Option<String>,
    pub tool_response: Option<String>,
    pub retry_count: i32,
    pub created_at_epoch: i64,
    pub claimed_at_epoch: Option<i64>,
    pub completed_at_epoch: Option<i64>,
}

pub fn max_retry_count() -> i32 {
    std::env::var("OPENCODE_MEM_MAX_RETRY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3)
}

pub fn default_visibility_timeout_secs() -> i64 {
    std::env::var("OPENCODE_MEM_VISIBILITY_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub pending: u64,
    pub processing: u64,
    pub failed: u64,
    pub processed: u64,
}
