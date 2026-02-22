use async_trait::async_trait;
use opencode_mem_core::{Observation, SearchResult, SimilarMatch, UserPrompt};

use crate::error::StorageError;
use crate::pending_queue::{PaginatedResult, StorageStats};

/// User prompt operations.
#[async_trait]
pub trait PromptStore: Send + Sync {
    /// Save user prompt.
    async fn save_user_prompt(&self, prompt: &UserPrompt) -> Result<(), StorageError>;

    /// Get prompts with pagination.
    async fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>, StorageError>;

    /// Get prompt by ID.
    async fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>, StorageError>;

    /// Search prompts by text.
    async fn search_prompts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<UserPrompt>, StorageError>;
}

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

/// Text and hybrid search operations.
#[async_trait]
pub trait SearchStore: Send + Sync {
    /// Full-text search.
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, StorageError>;

    /// Hybrid search combining full-text and keyword matching.
    async fn hybrid_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError>;

    /// Search with optional filters for project, type, and date range.
    async fn search_with_filters(
        &self,
        query: Option<&str>,
        project: Option<&str>,
        obs_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError>;

    /// Get observations within a time range.
    async fn get_timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError>;

    /// Vector similarity search.
    async fn semantic_search(
        &self,
        query_vec: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError>;

    /// Hybrid search: full-text BM25 (50%) + vector cosine similarity (50%).
    async fn hybrid_search_v2(
        &self,
        query: &str,
        query_vec: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError>;

    /// Hybrid search with optional filters.
    #[allow(clippy::too_many_arguments)]
    async fn hybrid_search_v2_with_filters(
        &self,
        query: &str,
        query_vec: &[f32],
        project: Option<&str>,
        obs_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError>;
}

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
