use async_trait::async_trait;
use opencode_mem_core::{Observation, SimilarMatch};

use crate::error::StorageError;

/// Embedding storage operations.
#[async_trait]
pub trait EmbeddingStore: Send + Sync {
    /// Store an embedding vector for an observation.
    async fn store_embedding(
        &self,
        observation_id: &str,
        embedding: &[f32],
    ) -> Result<(), StorageError>;

    /// Get observations that don't have embeddings yet.
    async fn get_observations_without_embeddings(
        &self,
        limit: usize,
    ) -> Result<Vec<Observation>, StorageError>;

    /// Drop and recreate the embedding index, forcing re-embedding of all observations.
    async fn clear_embeddings(&self) -> Result<(), StorageError>;

    /// Find the most similar existing observation by cosine similarity.
    async fn find_similar(
        &self,
        embedding: &[f32],
        threshold: f32,
    ) -> Result<Option<SimilarMatch>, StorageError>;

    /// Find top-N similar observations above a similarity threshold.
    ///
    /// Returns matches ordered by similarity descending, up to `limit` results.
    /// Used for providing context to the LLM (lower threshold than dedup).
    async fn find_similar_many(
        &self,
        embedding: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Result<Vec<SimilarMatch>, StorageError>;

    /// Get embeddings for specific observation IDs.
    ///
    /// Returns `(observation_id, embedding_vector)` pairs for IDs that have embeddings.
    async fn get_embeddings_for_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Vec<f32>)>, StorageError>;
}
