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
