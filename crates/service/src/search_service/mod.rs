//! Search service — read-only query facade over storage, embeddings, and hybrid search.

mod embedding_ops;

use std::sync::Arc;

use opencode_mem_core::{
    GlobalKnowledge, KnowledgeSearchResult, KnowledgeType, Observation, SearchResult,
    SessionSummary, UserPrompt,
};
use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_infinite::InfiniteMemory;
use opencode_mem_storage::traits::{
    KnowledgeStore, ObservationStore, PromptStore, SearchStore, StatsStore, SummaryStore,
};
use opencode_mem_storage::{
    CircuitBreaker, PaginatedResult, StorageBackend, StorageError, StorageStats,
};

use crate::ServiceError;

pub struct SearchService {
    pub(crate) storage: Arc<StorageBackend>,
    pub(crate) embeddings: Option<Arc<EmbeddingService>>,
    hybrid_search: opencode_mem_search::HybridSearch,
    infinite_mem: Option<Arc<InfiniteMemory>>,
}

impl SearchService {
    #[must_use]
    pub fn new(
        storage: Arc<StorageBackend>,
        embeddings: Option<Arc<EmbeddingService>>,
        infinite_mem: Option<Arc<InfiniteMemory>>,
    ) -> Self {
        let hybrid_search =
            opencode_mem_search::HybridSearch::new(storage.clone(), embeddings.clone());
        Self {
            storage,
            embeddings,
            hybrid_search,
            infinite_mem,
        }
    }

    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        self.storage.circuit_breaker()
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
            .hybrid_search
            .search_with_filters(query, project, obs_type, from, to, limit)
            .await
            .map_err(ServiceError::Search);
        self.with_cb(result).await
    }

    pub async fn hybrid_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .hybrid_search
            .search(query, limit)
            .await
            .map_err(ServiceError::Search);
        self.with_cb(result).await
    }

    pub async fn get_timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .get_timeline(from, to, limit)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn semantic_search_with_fallback(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .hybrid_search
            .semantic_search_with_fallback(query, limit)
            .await
            .map_err(ServiceError::Search);
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
        let result = self
            .storage
            .get_observations_paginated(offset, limit, project)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn search_sessions(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SessionSummary>, ServiceError> {
        let result = self
            .storage
            .search_sessions(query, limit)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_session_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionSummary>, ServiceError> {
        let result = self
            .storage
            .get_session_summary(session_id)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_summaries_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<SessionSummary>, ServiceError> {
        let result = self
            .storage
            .get_summaries_paginated(offset, limit, project)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn search_prompts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<UserPrompt>, ServiceError> {
        let result = self
            .storage
            .search_prompts(query, limit)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>, ServiceError> {
        let result = self
            .storage
            .get_prompt_by_id(id)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>, ServiceError> {
        let result = self
            .storage
            .get_prompts_paginated(offset, limit, project)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>, ServiceError> {
        let result = self
            .storage
            .search_knowledge(query, limit)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>, ServiceError> {
        let result = self
            .storage
            .list_knowledge(knowledge_type, limit)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>, ServiceError> {
        let result = self
            .storage
            .get_knowledge(id)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }
}
