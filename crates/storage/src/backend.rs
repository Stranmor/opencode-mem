//! Unified storage backend with enum dispatch.

#[cfg(feature = "sqlite")]
use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use opencode_mem_core::{
    GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType, Observation,
    SearchResult, Session, SessionStatus, SessionSummary, UserPrompt,
};

use crate::pending_queue::{PaginatedResult, PendingMessage, QueueStats, StorageStats};
use crate::traits::{
    EmbeddingStore, KnowledgeStore, ObservationStore, PendingQueueStore, PromptStore, SearchStore,
    SessionStore, StatsStore, SummaryStore,
};

macro_rules! dispatch {
    ($self:expr, $trait:path, $method:ident ( $($arg:expr),* $(,)? )) => {
        match $self {
            #[cfg(feature = "sqlite")]
            StorageBackend::Sqlite(s) => <crate::Storage as $trait>::$method(s, $($arg),*).await,
            #[cfg(feature = "postgres")]
            StorageBackend::Postgres(s) => <crate::pg_storage::PgStorage as $trait>::$method(s, $($arg),*).await,
        }
    };
}

#[derive(Clone, Debug)]
pub enum StorageBackend {
    #[cfg(feature = "sqlite")]
    Sqlite(crate::Storage),
    #[cfg(feature = "postgres")]
    Postgres(crate::pg_storage::PgStorage),
}

impl StorageBackend {
    #[cfg(feature = "sqlite")]
    pub fn new_sqlite(db_path: &Path) -> Result<Self> {
        Ok(Self::Sqlite(crate::Storage::new(db_path)?))
    }

    #[cfg(feature = "postgres")]
    pub async fn new_postgres(database_url: &str) -> Result<Self> {
        Ok(Self::Postgres(crate::pg_storage::PgStorage::new(database_url).await?))
    }
}

// ── ObservationStore ─────────────────────────────────────────────

#[async_trait]
impl ObservationStore for StorageBackend {
    async fn save_observation(&self, obs: &Observation) -> Result<bool> {
        dispatch!(self, ObservationStore, save_observation(obs))
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<Observation>> {
        dispatch!(self, ObservationStore, get_by_id(id))
    }

    async fn get_recent(&self, limit: usize) -> Result<Vec<SearchResult>> {
        dispatch!(self, ObservationStore, get_recent(limit))
    }

    async fn get_session_observations(&self, session_id: &str) -> Result<Vec<Observation>> {
        dispatch!(self, ObservationStore, get_session_observations(session_id))
    }

    async fn get_observations_by_ids(&self, ids: &[String]) -> Result<Vec<Observation>> {
        dispatch!(self, ObservationStore, get_observations_by_ids(ids))
    }

    async fn get_context_for_project(
        &self,
        project: &str,
        limit: usize,
    ) -> Result<Vec<Observation>> {
        dispatch!(self, ObservationStore, get_context_for_project(project, limit))
    }

    async fn get_session_observation_count(&self, session_id: &str) -> Result<usize> {
        dispatch!(self, ObservationStore, get_session_observation_count(session_id))
    }

    async fn search_by_file(&self, file_path: &str, limit: usize) -> Result<Vec<SearchResult>> {
        dispatch!(self, ObservationStore, search_by_file(file_path, limit))
    }
}

// ── SessionStore ─────────────────────────────────────────────────

#[async_trait]
impl SessionStore for StorageBackend {
    async fn save_session(&self, session: &Session) -> Result<()> {
        dispatch!(self, SessionStore, save_session(session))
    }

    async fn get_session(&self, id: &str) -> Result<Option<Session>> {
        dispatch!(self, SessionStore, get_session(id))
    }

    async fn get_session_by_content_id(&self, content_session_id: &str) -> Result<Option<Session>> {
        dispatch!(self, SessionStore, get_session_by_content_id(content_session_id))
    }

    async fn update_session_status(&self, id: &str, status: SessionStatus) -> Result<()> {
        dispatch!(self, SessionStore, update_session_status(id, status))
    }

    async fn delete_session(&self, session_id: &str) -> Result<bool> {
        dispatch!(self, SessionStore, delete_session(session_id))
    }

    async fn close_stale_sessions(&self, max_age_hours: i64) -> Result<usize> {
        dispatch!(self, SessionStore, close_stale_sessions(max_age_hours))
    }
}

// ── KnowledgeStore ───────────────────────────────────────────────

#[async_trait]
impl KnowledgeStore for StorageBackend {
    async fn save_knowledge(&self, input: KnowledgeInput) -> Result<GlobalKnowledge> {
        dispatch!(self, KnowledgeStore, save_knowledge(input))
    }

    async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>> {
        dispatch!(self, KnowledgeStore, get_knowledge(id))
    }

    async fn delete_knowledge(&self, id: &str) -> Result<bool> {
        dispatch!(self, KnowledgeStore, delete_knowledge(id))
    }

    async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>> {
        dispatch!(self, KnowledgeStore, search_knowledge(query, limit))
    }

    async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>> {
        dispatch!(self, KnowledgeStore, list_knowledge(knowledge_type, limit))
    }

    async fn update_knowledge_usage(&self, id: &str) -> Result<()> {
        dispatch!(self, KnowledgeStore, update_knowledge_usage(id))
    }
}

// ── SummaryStore ─────────────────────────────────────────────────

#[async_trait]
impl SummaryStore for StorageBackend {
    async fn save_summary(&self, summary: &SessionSummary) -> Result<()> {
        dispatch!(self, SummaryStore, save_summary(summary))
    }

    async fn get_session_summary(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        dispatch!(self, SummaryStore, get_session_summary(session_id))
    }

    async fn update_session_status_with_summary(
        &self,
        session_id: &str,
        status: SessionStatus,
        summary: Option<&str>,
    ) -> Result<()> {
        dispatch!(
            self,
            SummaryStore,
            update_session_status_with_summary(session_id, status, summary)
        )
    }

    async fn get_summaries_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<SessionSummary>> {
        dispatch!(self, SummaryStore, get_summaries_paginated(offset, limit, project))
    }

    async fn search_sessions(&self, query: &str, limit: usize) -> Result<Vec<SessionSummary>> {
        dispatch!(self, SummaryStore, search_sessions(query, limit))
    }
}

// ── PendingQueueStore ────────────────────────────────────────────

#[async_trait]
impl PendingQueueStore for StorageBackend {
    async fn queue_message(
        &self,
        session_id: &str,
        tool_name: Option<&str>,
        tool_input: Option<&str>,
        tool_response: Option<&str>,
        project: Option<&str>,
    ) -> Result<i64> {
        dispatch!(
            self,
            PendingQueueStore,
            queue_message(session_id, tool_name, tool_input, tool_response, project)
        )
    }

    async fn claim_pending_messages(
        &self,
        limit: usize,
        visibility_timeout_secs: i64,
    ) -> Result<Vec<PendingMessage>> {
        dispatch!(self, PendingQueueStore, claim_pending_messages(limit, visibility_timeout_secs))
    }

    async fn complete_message(&self, id: i64) -> Result<()> {
        dispatch!(self, PendingQueueStore, complete_message(id))
    }

    async fn fail_message(&self, id: i64, increment_retry: bool) -> Result<()> {
        dispatch!(self, PendingQueueStore, fail_message(id, increment_retry))
    }

    async fn get_pending_count(&self) -> Result<usize> {
        dispatch!(self, PendingQueueStore, get_pending_count())
    }

    async fn release_stale_messages(&self, visibility_timeout_secs: i64) -> Result<usize> {
        dispatch!(self, PendingQueueStore, release_stale_messages(visibility_timeout_secs))
    }

    async fn get_failed_messages(&self, limit: usize) -> Result<Vec<PendingMessage>> {
        dispatch!(self, PendingQueueStore, get_failed_messages(limit))
    }

    async fn get_all_pending_messages(&self, limit: usize) -> Result<Vec<PendingMessage>> {
        dispatch!(self, PendingQueueStore, get_all_pending_messages(limit))
    }

    async fn get_queue_stats(&self) -> Result<QueueStats> {
        dispatch!(self, PendingQueueStore, get_queue_stats())
    }

    async fn clear_failed_messages(&self) -> Result<usize> {
        dispatch!(self, PendingQueueStore, clear_failed_messages())
    }

    async fn retry_failed_messages(&self) -> Result<usize> {
        dispatch!(self, PendingQueueStore, retry_failed_messages())
    }

    async fn clear_all_pending_messages(&self) -> Result<usize> {
        dispatch!(self, PendingQueueStore, clear_all_pending_messages())
    }
}

// ── PromptStore ──────────────────────────────────────────────────

#[async_trait]
impl PromptStore for StorageBackend {
    async fn save_user_prompt(&self, prompt: &UserPrompt) -> Result<()> {
        dispatch!(self, PromptStore, save_user_prompt(prompt))
    }

    async fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>> {
        dispatch!(self, PromptStore, get_prompts_paginated(offset, limit, project))
    }

    async fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>> {
        dispatch!(self, PromptStore, get_prompt_by_id(id))
    }

    async fn search_prompts(&self, query: &str, limit: usize) -> Result<Vec<UserPrompt>> {
        dispatch!(self, PromptStore, search_prompts(query, limit))
    }
}

// ── StatsStore ───────────────────────────────────────────────────

#[async_trait]
impl StatsStore for StorageBackend {
    async fn get_stats(&self) -> Result<StorageStats> {
        dispatch!(self, StatsStore, get_stats())
    }

    async fn get_all_projects(&self) -> Result<Vec<String>> {
        dispatch!(self, StatsStore, get_all_projects())
    }

    async fn get_observations_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<Observation>> {
        dispatch!(self, StatsStore, get_observations_paginated(offset, limit, project))
    }
}

// ── SearchStore ──────────────────────────────────────────────────

#[async_trait]
impl SearchStore for StorageBackend {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        dispatch!(self, SearchStore, search(query, limit))
    }

    async fn hybrid_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        dispatch!(self, SearchStore, hybrid_search(query, limit))
    }

    async fn search_with_filters(
        &self,
        query: Option<&str>,
        project: Option<&str>,
        obs_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        dispatch!(self, SearchStore, search_with_filters(query, project, obs_type, from, to, limit))
    }

    async fn get_timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        dispatch!(self, SearchStore, get_timeline(from, to, limit))
    }

    async fn semantic_search(&self, query_vec: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        dispatch!(self, SearchStore, semantic_search(query_vec, limit))
    }

    async fn hybrid_search_v2(
        &self,
        query: &str,
        query_vec: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        dispatch!(self, SearchStore, hybrid_search_v2(query, query_vec, limit))
    }
}

// ── EmbeddingStore ───────────────────────────────────────────────

#[async_trait]
impl EmbeddingStore for StorageBackend {
    async fn store_embedding(&self, observation_id: &str, embedding: &[f32]) -> Result<()> {
        dispatch!(self, EmbeddingStore, store_embedding(observation_id, embedding))
    }

    async fn get_observations_without_embeddings(&self, limit: usize) -> Result<Vec<Observation>> {
        dispatch!(self, EmbeddingStore, get_observations_without_embeddings(limit))
    }
}
