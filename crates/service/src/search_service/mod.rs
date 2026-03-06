//! Search service — read-only query facade over storage and embeddings.

mod embedding_ops;
mod hybrid_ops;
mod query_ops;

use std::sync::Arc;

use opencode_mem_core::{Observation, SearchResult, cap_query_limit};
use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_storage::traits::{ObservationStore, SearchStore, StatsStore};
use opencode_mem_storage::{
    CircuitBreaker, PaginatedResult, StorageBackend, StorageError, StorageStats,
};

use crate::InfiniteMemoryService;
use crate::ServiceError;

pub struct SearchService {
    pub(crate) storage: Arc<StorageBackend>,
    pub(crate) embeddings: Option<Arc<EmbeddingService>>,
    infinite_mem: Option<Arc<InfiniteMemoryService>>,
}

impl SearchService {
    #[must_use]
    pub fn new(
        storage: Arc<StorageBackend>,
        embeddings: Option<Arc<EmbeddingService>>,
        infinite_mem: Option<Arc<InfiniteMemoryService>>,
    ) -> Self {
        Self {
            storage,
            embeddings,
            infinite_mem,
        }
    }

    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        self.storage.circuit_breaker()
    }

    pub(crate) fn normalize_limit(limit: usize) -> usize {
        cap_query_limit(limit)
    }

    pub(crate) fn fast_fail_if_db_unavailable(&self) -> Result<(), ServiceError> {
        let cb = self.storage.circuit_breaker();
        if cb.should_allow() {
            Ok(())
        } else {
            Err(ServiceError::Storage(StorageError::Unavailable {
                seconds_until_probe: cb.seconds_until_probe(),
            }))
        }
    }

    pub(crate) async fn with_cb<T>(
        &self,
        result: Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        match &result {
            Ok(_) => {
                let recovered = self.storage.circuit_breaker().record_success();
                if recovered {
                    self.handle_recovery();
                }
            }
            Err(e) if e.is_db_unavailable() => self.storage.circuit_breaker().record_failure(),
            Err(_) => {}
        }
        result
    }

    pub fn handle_recovery(&self) {
        if self.storage.has_pending_migrations() {
            let storage = self.storage.clone();
            tokio::spawn(async move {
                let _ = storage.try_run_migrations().await;
            });
        }

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
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .get_timeline(from, to, limit)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_observation_by_id(
        &self,
        id: &str,
    ) -> Result<Option<Observation>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self.storage.get_by_id(id).await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_recent_observations(
        &self,
        limit: usize,
    ) -> Result<Vec<Observation>, ServiceError> {
        let limit = Self::normalize_limit(limit);
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .get_recent(limit)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_observations_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<Observation>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .get_observations_by_ids(ids)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_context_for_project(
        &self,
        project: &str,
        limit: usize,
    ) -> Result<Vec<Observation>, ServiceError> {
        let limit = Self::normalize_limit(limit);
        let result = self
            .storage
            .get_context_for_project(project, limit)
            .await
            .map_err(ServiceError::from);
        let observations = self.with_cb(result).await?;
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
            .search_by_file(file_path, limit)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_stats(&self) -> Result<StorageStats, ServiceError> {
        let result = self.storage.get_stats().await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_all_projects(&self) -> Result<Vec<String>, ServiceError> {
        let result = self
            .storage
            .get_all_projects()
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
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
            .get_observations_paginated(offset, limit, project)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }
}

#[cfg(test)]
mod breaker_tests;
