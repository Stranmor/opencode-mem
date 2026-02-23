mod fts;
mod hybrid;
mod semantic;
mod timeline;
pub(crate) mod utils;

use crate::error::StorageError;
use crate::traits::SearchStore;
use async_trait::async_trait;
use opencode_mem_core::SearchResult;

use super::PgStorage;

#[async_trait]
impl SearchStore for PgStorage {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, StorageError> {
        fts::search(self, query, limit).await
    }

    async fn hybrid_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        hybrid::hybrid_search(self, query, limit).await
    }

    async fn search_with_filters(
        &self,
        query: Option<&str>,
        project: Option<&str>,
        obs_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        fts::search_with_filters(self, query, project, obs_type, from, to, limit).await
    }

    async fn get_timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        timeline::get_timeline(self, from, to, limit).await
    }

    async fn semantic_search(
        &self,
        query_vec: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        semantic::semantic_search(self, query_vec, limit).await
    }

    async fn hybrid_search_v2(
        &self,
        query: &str,
        query_vec: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        hybrid::hybrid_search_v2(self, query, query_vec, limit).await
    }

    async fn hybrid_search_v2_with_filters(
        &self,
        query: &str,
        query_vec: &[f32],
        project: Option<&str>,
        obs_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        hybrid::hybrid_search_v2_with_filters(
            self, query, query_vec, project, obs_type, from, to, limit,
        )
        .await
    }
}
