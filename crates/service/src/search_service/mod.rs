//! Search service — read-only query facade over storage and embeddings.

mod embedding_ops;
mod hybrid_ops;
mod query_ops;

use std::sync::Arc;

use opencode_mem_core::{Observation, SearchResult, cap_query_limit};
use opencode_mem_embeddings::LazyEmbeddingService;
use opencode_mem_storage::traits::{ObservationStore, SearchStore, StatsStore};
use opencode_mem_storage::{
    CircuitBreaker, PaginatedResult, StorageBackend, StorageError, StorageStats,
};

use crate::InfiniteMemoryService;
use crate::ServiceError;

pub struct SearchService {
    pub(crate) storage: Arc<StorageBackend>,
    pub(crate) embeddings: Option<Arc<LazyEmbeddingService>>,
    infinite_mem: Option<Arc<InfiniteMemoryService>>,
    pub(crate) dedup_threshold: f32,
}

impl SearchService {
    #[must_use]
    pub fn new(
        storage: Arc<StorageBackend>,
        embeddings: Option<Arc<LazyEmbeddingService>>,
        infinite_mem: Option<Arc<InfiniteMemoryService>>,
        dedup_threshold: f32,
    ) -> Self {
        Self {
            storage,
            embeddings,
            infinite_mem,
            dedup_threshold,
        }
    }

    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        self.storage.circuit_breaker()
    }

    pub(crate) fn normalize_limit(limit: usize) -> usize {
        cap_query_limit(limit)
    }

    pub(crate) fn with_cb<T>(&self, result: Result<T, StorageError>) -> Result<T, ServiceError> {
        result.map_err(ServiceError::from)
    }

    pub fn handle_recovery(&self) {
        self.storage.handle_recovery_static();

        if let Some(ref im) = self.infinite_mem
            && im.has_pending_migrations()
        {
            let im = Arc::clone(im);
            tokio::spawn(async move {
                im.try_run_migrations().await;
            });
        }
    }

    pub async fn get_timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        let limit = Self::normalize_limit(limit);
        let result = self
            .storage
            .guarded(|| self.storage.get_timeline(from, to, limit))
            .await;
        self.with_cb(result)
    }

    pub async fn get_observation_by_id(
        &self,
        id: &str,
    ) -> Result<Option<Observation>, ServiceError> {
        let result = self.storage.guarded(|| self.storage.get_by_id(id)).await;
        self.with_cb(result)
    }

    pub async fn get_recent_observations(
        &self,
        limit: usize,
    ) -> Result<Vec<Observation>, ServiceError> {
        let limit = Self::normalize_limit(limit);
        let result = self
            .storage
            .guarded(|| self.storage.get_recent(limit))
            .await;
        self.with_cb(result)
    }

    pub async fn get_observations_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<Observation>, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.get_observations_by_ids(ids))
            .await;
        self.with_cb(result)
    }

    pub async fn get_context_for_project(
        &self,
        project: &str,
        limit: usize,
    ) -> Result<Vec<Observation>, ServiceError> {
        let limit = Self::normalize_limit(limit);
        let result = self
            .storage
            .guarded(|| self.storage.get_context_for_project(project, limit))
            .await;
        let observations = self.with_cb(result)?;
        self.deduplicate_by_embedding(observations).await
    }

    pub async fn search_by_file(
        &self,
        file_path: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        let limit = Self::normalize_limit(limit);
        let result = self
            .storage
            .guarded(|| self.storage.search_by_file(file_path, limit))
            .await;
        self.with_cb(result)
    }

    pub async fn get_stats(&self) -> Result<StorageStats, ServiceError> {
        let result = self.storage.guarded(|| self.storage.get_stats()).await;
        self.with_cb(result)
    }

    pub async fn get_all_projects(&self) -> Result<Vec<String>, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.get_all_projects())
            .await;
        self.with_cb(result)
    }

    pub async fn get_observations_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<Observation>, ServiceError> {
        let limit = Self::normalize_limit(limit);
        let result = self
            .storage
            .guarded(|| {
                self.storage
                    .get_observations_paginated(offset, limit, project)
            })
            .await;
        self.with_cb(result)
    }
}
