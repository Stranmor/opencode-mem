use async_trait::async_trait;
use opencode_mem_core::{GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType};

use crate::error::StorageError;

/// Knowledge base operations.
#[async_trait]
pub trait KnowledgeStore: Send + Sync {
    /// Save or update a knowledge entry (upserts by title).
    async fn save_knowledge(&self, input: KnowledgeInput) -> Result<GlobalKnowledge, StorageError>;

    /// Get knowledge entry by ID.
    async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>, StorageError>;

    /// Delete knowledge entry by ID. Returns `true` if deleted.
    async fn delete_knowledge(&self, id: &str) -> Result<bool, StorageError>;

    /// Check if knowledge exists for an observation.
    async fn has_knowledge_for_observation(
        &self,
        observation_id: &str,
    ) -> Result<bool, StorageError>;

    /// Full-text search over knowledge.
    async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>, StorageError>;

    /// List knowledge entries, optionally filtered by type.
    async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>, StorageError>;

    /// Increment usage count and bump confidence.
    async fn update_knowledge_usage(&self, id: &str) -> Result<(), StorageError>;
}
