//! Hybrid search operations — combines FTS (tsvector) and vector similarity (pgvector).
//!
//! Inlined from the former `opencode-mem-search` crate. The routing logic
//! (embed query → choose hybrid_search_v2 or fallback) now lives directly
//! in `SearchService`, eliminating the `anyhow::Result` type-erasure layer.

use std::sync::Arc;

use opencode_mem_core::SearchResult;
use opencode_mem_embeddings::{EmbeddingProvider, EmbeddingService};
use opencode_mem_storage::traits::SearchStore;

use crate::ServiceError;

use super::SearchService;

impl SearchService {
    /// Hybrid search: FTS + optional vector similarity.
    ///
    /// When embeddings are available, generates query embedding and uses
    /// `hybrid_search_v2` (50% FTS BM25 + 50% vector cosine similarity).
    /// Otherwise falls back to text-only `hybrid_search` (70% FTS + 30% keyword overlap).
    pub async fn hybrid_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self.run_hybrid_search(query, limit).await;
        self.with_cb(result).await
    }

    /// Search with additional filters (project, observation type, date range).
    pub async fn search_with_filters(
        &self,
        query: Option<&str>,
        project: Option<&str>,
        obs_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .run_search_with_filters(query, project, obs_type, from, to, limit)
            .await;
        self.with_cb(result).await
    }

    /// Smart search: selects the best strategy based on available parameters.
    ///
    /// When no filters are applied and a query string is present, uses hybrid
    /// search (FTS + vector). Otherwise falls back to filtered search. This
    /// encapsulates the search strategy decision so both HTTP and MCP transports
    /// call the same logic.
    pub async fn smart_search(
        &self,
        query: Option<&str>,
        project: Option<&str>,
        obs_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        let has_filters = project.is_some() || obs_type.is_some() || from.is_some() || to.is_some();
        if !has_filters && let Some(q) = query.filter(|s| !s.is_empty()) {
            return self.hybrid_search(q, limit).await;
        }
        self.search_with_filters(query, project, obs_type, from, to, limit)
            .await
    }

    /// Semantic search with automatic 3-tier fallback:
    /// 1. Vector search via embeddings
    /// 2. If vector results are empty → hybrid search
    /// 3. If embedding fails or unavailable → hybrid search
    pub async fn semantic_search_with_fallback(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self.run_semantic_search_with_fallback(query, limit).await;
        self.with_cb(result).await
    }

    // ── Private routing implementations ─────────────────────────────────

    async fn run_hybrid_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        if let Some(query_vec) = self.try_embed(query).await? {
            Ok(self
                .storage
                .hybrid_search_v2(query, &query_vec, limit)
                .await?)
        } else {
            Ok(self.storage.hybrid_search(query, limit).await?)
        }
    }

    async fn run_search_with_filters(
        &self,
        query: Option<&str>,
        project: Option<&str>,
        obs_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        if let Some(q) = query
            && let Some(query_vec) = self.try_embed(q).await?
        {
            return Ok(self
                .storage
                .hybrid_search_v2_with_filters(q, &query_vec, project, obs_type, from, to, limit)
                .await?);
        }
        Ok(self
            .storage
            .search_with_filters(query, project, obs_type, from, to, limit)
            .await?)
    }

    async fn run_semantic_search_with_fallback(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        let Some(ref emb) = self.embeddings else {
            return Ok(self.storage.hybrid_search(query, limit).await?);
        };

        let embed_result = embed_query(emb, query).await;

        match embed_result {
            Ok(query_vec) => match self.storage.semantic_search(&query_vec, limit).await {
                Ok(results) if !results.is_empty() => Ok(results),
                Ok(_) => Ok(self.storage.hybrid_search(query, limit).await?),
                Err(e) => {
                    tracing::warn!(
                        "Semantic search storage error, falling back to hybrid: {}",
                        e
                    );
                    Ok(self.storage.hybrid_search(query, limit).await?)
                }
            },
            Err(e) => {
                tracing::warn!("Failed to embed query, falling back to hybrid: {}", e);
                Ok(self.storage.hybrid_search(query, limit).await?)
            }
        }
    }

    /// Try to embed the query. Returns `Ok(None)` if embeddings are not configured.
    /// Returns `Ok(Some(vec))` on success, `Err` on embedding failure.
    async fn try_embed(&self, query: &str) -> Result<Option<Vec<f32>>, ServiceError> {
        let Some(ref emb) = self.embeddings else {
            return Ok(None);
        };
        let vec = embed_query(emb, query).await?;
        Ok(Some(vec))
    }
}

async fn embed_query(emb: &Arc<EmbeddingService>, query: &str) -> Result<Vec<f32>, ServiceError> {
    let emb_clone = emb.clone();
    let query_str = query.to_owned();
    let vec = tokio::task::spawn_blocking(move || emb_clone.embed(&query_str))
        .await
        .map_err(|e| {
            ServiceError::Embedding(opencode_mem_embeddings::error::EmbeddingError::Generation(
                e.to_string(),
            ))
        })??;
    Ok(vec)
}
