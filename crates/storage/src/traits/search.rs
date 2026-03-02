use async_trait::async_trait;
use opencode_mem_core::SearchResult;

use crate::error::StorageError;

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
