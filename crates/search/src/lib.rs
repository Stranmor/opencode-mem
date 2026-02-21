//! Hybrid search combining full-text search (tsvector) and vector similarity (pgvector)
//!
//! Implements 3-layer search pattern:
//! 1. search(query) → Get lightweight index with IDs and scores
//! 2. timeline(from/to) → Get context around interesting results
//! 3. `get_full(ids)` → Fetch full observations ONLY for filtered IDs

#![allow(clippy::missing_errors_doc, reason = "Errors are self-explanatory from Result types")]
#![allow(clippy::pattern_type_mismatch, reason = "Pattern matching style")]
#![allow(missing_debug_implementations, reason = "Internal types")]
#![allow(clippy::missing_docs_in_private_items, reason = "Internal crate")]
#![allow(clippy::implicit_return, reason = "Implicit return is idiomatic Rust")]
#![allow(clippy::question_mark_used, reason = "? operator is idiomatic Rust")]

use std::sync::Arc;

use anyhow::Result;
use opencode_mem_core::{Observation, SearchResult};
use opencode_mem_embeddings::{EmbeddingProvider, EmbeddingService};
use opencode_mem_storage::StorageBackend;
use opencode_mem_storage::traits::{ObservationStore, SearchStore};

/// High-level search facade combining full-text search (tsvector) and vector similarity (pgvector).
///
/// Wraps `StorageBackend` and optional `EmbeddingService` to provide unified search API.
/// When embeddings are available, uses `hybrid_search_v2` (FTS + vector).
/// Otherwise falls back to text-only `hybrid_search`.
pub struct HybridSearch {
    storage: Arc<StorageBackend>,
    embeddings: Option<Arc<EmbeddingService>>,
}

impl HybridSearch {
    /// Create new `HybridSearch` instance.
    ///
    /// # Arguments
    /// * `storage` - Storage backend for database operations
    /// * `embeddings` - Optional embedding service for semantic search
    #[must_use]
    pub fn new(storage: Arc<StorageBackend>, embeddings: Option<Arc<EmbeddingService>>) -> Self {
        Self { storage, embeddings }
    }

    /// Step 1: Search and return lightweight index results.
    ///
    /// Returns `SearchResult` with id, title, subtitle, type, and relevance score.
    /// Use `get_full()` to fetch complete observations for selected results.
    ///
    /// If embeddings are available, generates query embedding and uses
    /// `hybrid_search_v2` (50% FTS BM25 + 50% vector cosine similarity).
    /// Otherwise falls back to text-only `hybrid_search` (70% FTS + 30% keyword overlap).
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        match &self.embeddings {
            Some(emb) => {
                let emb_clone = emb.clone();
                let query_str = query.to_owned();
                let query_vec =
                    tokio::task::spawn_blocking(move || emb_clone.embed(&query_str)).await??;
                Ok(self.storage.hybrid_search_v2(query, &query_vec, limit).await?)
            },
            None => Ok(self.storage.hybrid_search(query, limit).await?),
        }
    }

    /// Step 2: Get timeline context around a time range.
    ///
    /// Returns observations within the specified time range, ordered by creation time.
    /// Useful for getting context around interesting search results.
    ///
    /// # Arguments
    /// * `from` - Optional start time (ISO 8601 format)
    /// * `to` - Optional end time (ISO 8601 format)
    /// * `limit` - Maximum number of results
    pub async fn timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        self.storage.get_timeline(from, to, limit).await.map_err(Into::into)
    }

    /// Step 3: Fetch full observations by IDs.
    ///
    /// After filtering search results, use this to get complete observation data
    /// including narrative, facts, concepts, files, etc.
    ///
    /// # Arguments
    /// * `ids` - List of observation IDs to fetch
    pub async fn get_full(&self, ids: &[String]) -> Result<Vec<Observation>> {
        self.storage.get_observations_by_ids(ids).await.map_err(Into::into)
    }

    /// Get recent observations (convenience method).
    ///
    /// Returns the most recent observations without any search query.
    pub async fn recent(&self, limit: usize) -> Result<Vec<Observation>> {
        self.storage.get_recent(limit).await.map_err(Into::into)
    }

    /// Search with additional filters (project, observation type, date range).
    ///
    /// # Arguments
    /// * `query` - Optional search query
    /// * `project` - Optional project filter
    /// * `obs_type` - Optional observation type filter
    /// * `from` - Optional start date (ISO 8601)
    /// * `to` - Optional end date (ISO 8601)
    /// * `limit` - Maximum number of results
    pub async fn search_with_filters(
        &self,
        query: Option<&str>,
        project: Option<&str>,
        obs_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        self.storage
            .search_with_filters(query, project, obs_type, from, to, limit)
            .await
            .map_err(Into::into)
    }

    /// Pure semantic search using vector similarity only.
    ///
    /// Returns None if embeddings are not available.
    /// Use `search()` for hybrid FTS + semantic search.
    pub async fn semantic_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Option<Vec<SearchResult>>> {
        match &self.embeddings {
            Some(emb) => {
                let emb_clone = emb.clone();
                let query_str = query.to_owned();
                let query_vec =
                    tokio::task::spawn_blocking(move || emb_clone.embed(&query_str)).await??;
                let results = self.storage.semantic_search(&query_vec, limit).await?;
                Ok(Some(results))
            },
            None => Ok(None),
        }
    }

    /// Semantic search with automatic fallback to hybrid search.
    ///
    /// Implements 3-tier fallback chain:
    /// 1. Try vector search via embeddings
    /// 2. If vector results are empty → fallback to hybrid search
    /// 3. If embedding fails → fallback to hybrid search
    /// 4. If no embeddings service → use hybrid search directly
    ///
    /// Guarantees a result regardless of embeddings availability.
    pub async fn semantic_search_with_fallback(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        run_semantic_search_with_fallback(&self.storage, self.embeddings.as_ref(), query, limit)
            .await
    }

    /// Check if semantic search is available.
    #[must_use]
    pub const fn has_embeddings(&self) -> bool {
        self.embeddings.is_some()
    }

    /// Get a single observation by ID.
    pub async fn get_by_id(&self, id: &str) -> Result<Option<Observation>> {
        self.storage.get_by_id(id).await.map_err(Into::into)
    }

    /// Search by file path.
    ///
    /// Finds observations that mention the given file path in `files_read` or `files_modified`.
    pub async fn search_by_file(&self, file_path: &str, limit: usize) -> Result<Vec<SearchResult>> {
        self.storage.search_by_file(file_path, limit).await.map_err(Into::into)
    }
}

/// Semantic search with 3-tier fallback, usable without constructing `HybridSearch`.
///
/// Called by both HTTP and MCP handlers that have `&StorageBackend` references
/// rather than `Arc<StorageBackend>`.
pub async fn run_semantic_search_with_fallback(
    storage: &StorageBackend,
    embeddings: Option<&Arc<EmbeddingService>>,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>> {
    match embeddings {
        Some(emb) => {
            let emb_clone = emb.clone();
            let query_str = query.to_owned();
            let embed_result =
                tokio::task::spawn_blocking(move || emb_clone.embed(&query_str)).await?;

            match embed_result {
                Ok(query_vec) => match storage.semantic_search(&query_vec, limit).await {
                    Ok(results) if !results.is_empty() => Ok(results),
                    Ok(_) => storage.hybrid_search(query, limit).await.map_err(Into::into),
                    Err(e) => {
                        tracing::warn!(
                            "Semantic search storage error, falling back to hybrid: {}",
                            e
                        );
                        storage.hybrid_search(query, limit).await.map_err(Into::into)
                    },
                },
                Err(e) => {
                    tracing::warn!("Failed to embed query, falling back to hybrid: {}", e);
                    storage.hybrid_search(query, limit).await.map_err(Into::into)
                },
            }
        },
        None => storage.hybrid_search(query, limit).await.map_err(Into::into),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[expect(clippy::expect_used, reason = "test code")]
    async fn create_test_storage() -> Arc<StorageBackend> {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
        let storage = StorageBackend::new(&url).await.expect("Failed to connect to PG");
        Arc::new(storage)
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_hybrid_search_creation() {
        let storage = create_test_storage().await;
        let search = HybridSearch::new(storage, None);
        assert!(!search.has_embeddings());
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    #[expect(clippy::expect_used, reason = "test code")]
    async fn test_search_without_embeddings() {
        let storage = create_test_storage().await;
        let search = HybridSearch::new(storage, None);

        // Should not panic, returns empty results
        let results = search.search("test query", 10).await.expect("Search failed");
        assert!(results.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    #[expect(clippy::expect_used, reason = "test code")]
    async fn test_timeline_empty() {
        let storage = create_test_storage().await;
        let search = HybridSearch::new(storage, None);

        let results = search.timeline(None, None, 10).await.expect("Timeline failed");
        assert!(results.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    #[expect(clippy::expect_used, reason = "test code")]
    async fn test_get_full_empty() {
        let storage = create_test_storage().await;
        let search = HybridSearch::new(storage, None);

        let results = search.get_full(&[]).await.expect("Get full failed");
        assert!(results.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    #[expect(clippy::expect_used, reason = "test code")]
    async fn test_semantic_search_without_embeddings() {
        let storage = create_test_storage().await;
        let search = HybridSearch::new(storage, None);

        let result = search.semantic_search("test", 10).await.expect("Semantic search failed");
        assert!(result.is_none());
    }
}
