use std::sync::Arc;

use opencode_mem_core::{
    GlobalKnowledge, KnowledgeSearchResult, KnowledgeType, Observation, SearchResult,
    SessionSummary, UserPrompt,
};
use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_storage::traits::{
    EmbeddingStore, KnowledgeStore, ObservationStore, PromptStore, SearchStore, StatsStore,
    SummaryStore,
};
use opencode_mem_storage::{PaginatedResult, StorageBackend, StorageStats};

use crate::ServiceError;

pub struct SearchService {
    storage: Arc<StorageBackend>,
    embeddings: Option<Arc<EmbeddingService>>,
}

impl SearchService {
    #[must_use]
    pub fn new(storage: Arc<StorageBackend>, embeddings: Option<Arc<EmbeddingService>>) -> Self {
        Self { storage, embeddings }
    }

    // ── SearchStore delegates ──────────────────────────────────────────

    pub async fn search_with_filters(
        &self,
        query: Option<&str>,
        project: Option<&str>,
        obs_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        Ok(self.storage.search_with_filters(query, project, obs_type, from, to, limit).await?)
    }

    pub async fn hybrid_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        Ok(self.storage.hybrid_search(query, limit).await?)
    }

    pub async fn get_timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        Ok(self.storage.get_timeline(from, to, limit).await?)
    }

    pub async fn semantic_search_with_fallback(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        opencode_mem_search::run_semantic_search_with_fallback(
            &self.storage,
            self.embeddings.as_deref(),
            query,
            limit,
        )
        .await
        .map_err(ServiceError::Search)
    }

    // ── ObservationStore read delegates ─────────────────────────────────

    pub async fn get_observation_by_id(
        &self,
        id: &str,
    ) -> Result<Option<Observation>, ServiceError> {
        Ok(self.storage.get_by_id(id).await?)
    }

    pub async fn get_recent_observations(
        &self,
        limit: usize,
    ) -> Result<Vec<Observation>, ServiceError> {
        Ok(self.storage.get_recent(limit).await?)
    }

    pub async fn get_observations_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<Observation>, ServiceError> {
        Ok(self.storage.get_observations_by_ids(ids).await?)
    }

    pub async fn get_context_for_project(
        &self,
        project: &str,
        limit: usize,
    ) -> Result<Vec<Observation>, ServiceError> {
        Ok(self.storage.get_context_for_project(project, limit).await?)
    }

    pub async fn search_by_file(
        &self,
        file_path: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        Ok(self.storage.search_by_file(file_path, limit).await?)
    }

    // ── StatsStore delegates ───────────────────────────────────────────

    pub async fn get_stats(&self) -> Result<StorageStats, ServiceError> {
        Ok(self.storage.get_stats().await?)
    }

    pub async fn get_all_projects(&self) -> Result<Vec<String>, ServiceError> {
        Ok(self.storage.get_all_projects().await?)
    }

    pub async fn get_observations_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<Observation>, ServiceError> {
        Ok(self.storage.get_observations_paginated(offset, limit, project).await?)
    }

    // ── SummaryStore delegates ─────────────────────────────────────────

    pub async fn search_sessions(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SessionSummary>, ServiceError> {
        Ok(self.storage.search_sessions(query, limit).await?)
    }

    pub async fn get_session_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionSummary>, ServiceError> {
        Ok(self.storage.get_session_summary(session_id).await?)
    }

    pub async fn get_summaries_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<SessionSummary>, ServiceError> {
        Ok(self.storage.get_summaries_paginated(offset, limit, project).await?)
    }

    // ── PromptStore delegates ──────────────────────────────────────────

    pub async fn search_prompts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<UserPrompt>, ServiceError> {
        Ok(self.storage.search_prompts(query, limit).await?)
    }

    pub async fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>, ServiceError> {
        Ok(self.storage.get_prompt_by_id(id).await?)
    }

    pub async fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>, ServiceError> {
        Ok(self.storage.get_prompts_paginated(offset, limit, project).await?)
    }

    // ── KnowledgeStore delegates (read-only) ───────────────────────────

    pub async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>, ServiceError> {
        Ok(self.storage.search_knowledge(query, limit).await?)
    }

    pub async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>, ServiceError> {
        Ok(self.storage.list_knowledge(knowledge_type, limit).await?)
    }

    pub async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>, ServiceError> {
        Ok(self.storage.get_knowledge(id).await?)
    }

    // ── EmbeddingStore delegates ────────────────────────────────────────

    pub async fn clear_embeddings(&self) -> Result<(), ServiceError> {
        Ok(self.storage.clear_embeddings().await?)
    }
}
