use async_trait::async_trait;
use opencode_mem_core::Observation;

use crate::error::StorageError;
use crate::pending_queue::{PaginatedResult, StorageStats};

/// Aggregate statistics.
#[async_trait]
pub trait StatsStore: Send + Sync {
    /// Get storage statistics.
    async fn get_stats(&self) -> Result<StorageStats, StorageError>;

    /// Get all distinct projects.
    async fn get_all_projects(&self) -> Result<Vec<String>, StorageError>;

    /// Get observations with pagination.
    async fn get_observations_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<Observation>, StorageError>;
}
