use async_trait::async_trait;
use opencode_mem_core::{GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType};

use crate::error::StorageError;

/// Knowledge base operations.
#[async_trait]
pub trait KnowledgeStore: Send + Sync {
    /// Save or update a knowledge entry (upserts by title).
    async fn save_knowledge(&self, input: KnowledgeInput) -> Result<GlobalKnowledge, StorageError>;

    /// Save or update a knowledge entry with a specific ID.
    async fn save_knowledge_with_id(
        &self,
        id: &str,
        input: KnowledgeInput,
    ) -> Result<GlobalKnowledge, StorageError>;

    /// Save or update a knowledge entry with semantic dedup via embedding.
    ///
    /// When provided, the embedding is used for cosine similarity search
    /// against existing knowledge before creating a new entry.
    async fn save_knowledge_with_embedding(
        &self,
        id: &str,
        input: KnowledgeInput,
        embedding: Vec<f32>,
    ) -> Result<GlobalKnowledge, StorageError>;

    /// Store embedding vector for a knowledge entry.
    async fn store_knowledge_embedding(
        &self,
        knowledge_id: &str,
        embedding: &[f32],
    ) -> Result<(), StorageError>;

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

    /// Increment usage count and bump confidence for a batch of entries.
    async fn update_knowledge_usage_batch(&self, ids: &[String]) -> Result<(), StorageError>;

    /// Decay confidence for all non-archived entries based on time since last use.
    /// Returns the number of entries updated.
    async fn decay_confidence(&self) -> Result<u64, StorageError>;

    /// Archive entries with low confidence, zero usage, and older than the given age in days.
    /// Returns the number of entries archived.
    async fn auto_archive(&self, min_age_days: i64) -> Result<u64, StorageError>;

    /// Append an observation ID to a knowledge entry's `source_observations` array
    /// if not already present. Returns `true` if the array was modified.
    async fn link_source_observation(
        &self,
        knowledge_id: &str,
        observation_id: &str,
    ) -> Result<bool, StorageError>;
}
