use async_trait::async_trait;
use opencode_mem_core::{Observation, ObservationMetadata, SearchResult};

use crate::error::StorageError;

/// CRUD operations on observations.
#[async_trait]
pub trait ObservationStore: Send + Sync {
    /// Save observation. Returns `true` if inserted, `false` on duplicate.
    async fn save_observation(&self, obs: &Observation) -> Result<bool, StorageError>;

    /// Get observation by ID.
    async fn get_by_id(&self, id: &str) -> Result<Option<Observation>, StorageError>;

    /// Get recent observations.
    async fn get_recent(&self, limit: usize) -> Result<Vec<Observation>, StorageError>;

    /// Get all observations for a session.
    async fn get_session_observations(
        &self,
        session_id: &str,
    ) -> Result<Vec<Observation>, StorageError>;
    /// Get the N most recent observations for a session (by created_at DESC).
    async fn get_recent_session_observations(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<Observation>, StorageError>;

    /// Get observations by a list of IDs.
    async fn get_observations_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<Observation>, StorageError>;

    /// Get observations for a project.
    async fn get_context_for_project(
        &self,
        project: &str,
        limit: usize,
    ) -> Result<Vec<Observation>, StorageError>;

    /// Count observations in a session.
    async fn get_session_observation_count(&self, session_id: &str) -> Result<usize, StorageError>;

    /// Search observations by file path.
    async fn search_by_file(
        &self,
        file_path: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError>;

    /// Merge two observations and purge the duplicate, repointing knowledge entries.
    async fn merge_and_purge(
        &self,
        keeper_id: &str,
        duplicate_id: &str,
    ) -> Result<(), StorageError>;

    /// Merge a newer observation data into an existing one (updates facts, keywords, etc.).
    ///
    /// If `force_newer` is true, the `newer` observation's fields (title, type, narrative, subtitle)
    /// unconditionally overwrite the `existing` ones.
    async fn merge_into_existing(
        &self,
        existing_id: &str,
        newer: &Observation,
        force_newer: bool,
    ) -> Result<(), StorageError>;

    /// Update only metadata fields (facts, concepts, keywords, files_read, files_modified)
    /// on an existing observation.
    async fn update_observation_metadata(
        &self,
        id: &str,
        metadata: &ObservationMetadata,
    ) -> Result<(), StorageError>;

    /// Get observations that have empty metadata (facts, concepts, keywords all empty).
    /// Used by the backfill-metadata CLI command to find observations needing enrichment.
    async fn get_observations_with_empty_metadata(
        &self,
        limit: usize,
        excluded_ids: &[String],
    ) -> Result<Vec<Observation>, StorageError>;
}
