use async_trait::async_trait;

use crate::error::StorageError;

/// Injection tracking for dedup (records which observations were injected into context).
#[async_trait]
pub trait InjectionStore: Send + Sync {
    /// Record that the given observation IDs were injected for a session.
    async fn save_injected_observations(
        &self,
        session_id: &str,
        observation_ids: &[String],
    ) -> Result<(), StorageError>;

    /// Get all injected observation IDs for a session.
    async fn get_injected_observation_ids(
        &self,
        session_id: &str,
    ) -> Result<Vec<String>, StorageError>;

    /// Delete injections older than `older_than_hours`. Returns count deleted.
    async fn cleanup_old_injections(&self, older_than_hours: u32) -> Result<u64, StorageError>;
}
