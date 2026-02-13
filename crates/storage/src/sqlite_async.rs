//! Async trait implementations for SQLite `Storage` via `spawn_blocking`.

use anyhow::Result;
use async_trait::async_trait;
use opencode_mem_core::{
    GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType, Observation,
    SearchResult, Session, SessionStatus, SessionSummary, SimilarMatch, UserPrompt,
};

use crate::pending_queue::{PaginatedResult, PendingMessage, QueueStats, StorageStats};
use crate::traits::{
    EmbeddingStore, KnowledgeStore, ObservationStore, PendingQueueStore, PromptStore, SearchStore,
    SessionStore, StatsStore, SummaryStore,
};
use crate::Storage;

/// Helper: run a blocking closure on the tokio blocking pool.
async fn blocking<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {e}"))?
}

// ── ObservationStore ─────────────────────────────────────────────

#[async_trait]
impl ObservationStore for Storage {
    async fn save_observation(&self, obs: &Observation) -> Result<bool> {
        let s = self.clone();
        let obs = obs.clone();
        blocking(move || s.save_observation(&obs)).await
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<Observation>> {
        let s = self.clone();
        let id = id.to_owned();
        blocking(move || s.get_by_id(&id)).await
    }

    async fn get_recent(&self, limit: usize) -> Result<Vec<SearchResult>> {
        let s = self.clone();
        blocking(move || s.get_recent(limit)).await
    }

    async fn get_session_observations(&self, session_id: &str) -> Result<Vec<Observation>> {
        let s = self.clone();
        let session_id = session_id.to_owned();
        blocking(move || s.get_session_observations(&session_id)).await
    }

    async fn get_observations_by_ids(&self, ids: &[String]) -> Result<Vec<Observation>> {
        let s = self.clone();
        let ids = ids.to_vec();
        blocking(move || s.get_observations_by_ids(&ids)).await
    }

    async fn get_context_for_project(
        &self,
        project: &str,
        limit: usize,
    ) -> Result<Vec<Observation>> {
        let s = self.clone();
        let project = project.to_owned();
        blocking(move || s.get_context_for_project(&project, limit)).await
    }

    async fn get_session_observation_count(&self, session_id: &str) -> Result<usize> {
        let s = self.clone();
        let session_id = session_id.to_owned();
        blocking(move || s.get_session_observation_count(&session_id)).await
    }

    async fn search_by_file(&self, file_path: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let s = self.clone();
        let file_path = file_path.to_owned();
        blocking(move || s.search_by_file(&file_path, limit)).await
    }

    async fn merge_into_existing(&self, existing_id: &str, newer: &Observation) -> Result<()> {
        let s = self.clone();
        let existing_id = existing_id.to_owned();
        let newer = newer.clone();
        blocking(move || s.merge_into_existing(&existing_id, &newer)).await
    }
}

// ── SessionStore ─────────────────────────────────────────────────

#[async_trait]
impl SessionStore for Storage {
    async fn save_session(&self, session: &Session) -> Result<()> {
        let s = self.clone();
        let session = session.clone();
        blocking(move || s.save_session(&session)).await
    }

    async fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let s = self.clone();
        let id = id.to_owned();
        blocking(move || s.get_session(&id)).await
    }

    async fn get_session_by_content_id(&self, content_session_id: &str) -> Result<Option<Session>> {
        let s = self.clone();
        let content_session_id = content_session_id.to_owned();
        blocking(move || s.get_session_by_content_id(&content_session_id)).await
    }

    async fn update_session_status(&self, id: &str, status: SessionStatus) -> Result<()> {
        let s = self.clone();
        let id = id.to_owned();
        blocking(move || s.update_session_status(&id, status)).await
    }

    async fn delete_session(&self, session_id: &str) -> Result<bool> {
        let s = self.clone();
        let session_id = session_id.to_owned();
        blocking(move || s.delete_session(&session_id)).await
    }

    async fn close_stale_sessions(&self, max_age_hours: i64) -> Result<usize> {
        let s = self.clone();
        blocking(move || s.close_stale_sessions(max_age_hours)).await
    }
}

// ── KnowledgeStore ───────────────────────────────────────────────

#[async_trait]
impl KnowledgeStore for Storage {
    async fn save_knowledge(&self, input: KnowledgeInput) -> Result<GlobalKnowledge> {
        let s = self.clone();
        blocking(move || s.save_knowledge(input)).await
    }

    async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>> {
        let s = self.clone();
        let id = id.to_owned();
        blocking(move || s.get_knowledge(&id)).await
    }

    async fn delete_knowledge(&self, id: &str) -> Result<bool> {
        let s = self.clone();
        let id = id.to_owned();
        blocking(move || s.delete_knowledge(&id)).await
    }

    async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>> {
        let s = self.clone();
        let query = query.to_owned();
        blocking(move || s.search_knowledge(&query, limit)).await
    }

    async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>> {
        let s = self.clone();
        blocking(move || s.list_knowledge(knowledge_type, limit)).await
    }

    async fn update_knowledge_usage(&self, id: &str) -> Result<()> {
        let s = self.clone();
        let id = id.to_owned();
        blocking(move || s.update_knowledge_usage(&id)).await
    }
}

// ── SummaryStore ─────────────────────────────────────────────────

#[async_trait]
impl SummaryStore for Storage {
    async fn save_summary(&self, summary: &SessionSummary) -> Result<()> {
        let s = self.clone();
        let summary = summary.clone();
        blocking(move || s.save_summary(&summary)).await
    }

    async fn get_session_summary(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        let s = self.clone();
        let session_id = session_id.to_owned();
        blocking(move || s.get_session_summary(&session_id)).await
    }

    async fn update_session_status_with_summary(
        &self,
        session_id: &str,
        status: SessionStatus,
        summary: Option<&str>,
    ) -> Result<()> {
        let s = self.clone();
        let session_id = session_id.to_owned();
        let summary = summary.map(ToOwned::to_owned);
        blocking(move || {
            s.update_session_status_with_summary(&session_id, status, summary.as_deref())
        })
        .await
    }

    async fn get_summaries_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<SessionSummary>> {
        let s = self.clone();
        let project = project.map(ToOwned::to_owned);
        blocking(move || s.get_summaries_paginated(offset, limit, project.as_deref())).await
    }

    async fn search_sessions(&self, query: &str, limit: usize) -> Result<Vec<SessionSummary>> {
        let s = self.clone();
        let query = query.to_owned();
        blocking(move || s.search_sessions(&query, limit)).await
    }
}

// ── PendingQueueStore ────────────────────────────────────────────

#[async_trait]
impl PendingQueueStore for Storage {
    async fn queue_message(
        &self,
        session_id: &str,
        tool_name: Option<&str>,
        tool_input: Option<&str>,
        tool_response: Option<&str>,
        project: Option<&str>,
    ) -> Result<i64> {
        let s = self.clone();
        let session_id = session_id.to_owned();
        let tool_name = tool_name.map(ToOwned::to_owned);
        let tool_input = tool_input.map(ToOwned::to_owned);
        let tool_response = tool_response.map(ToOwned::to_owned);
        let project = project.map(ToOwned::to_owned);
        blocking(move || {
            s.queue_message(
                &session_id,
                tool_name.as_deref(),
                tool_input.as_deref(),
                tool_response.as_deref(),
                project.as_deref(),
            )
        })
        .await
    }

    async fn claim_pending_messages(
        &self,
        limit: usize,
        visibility_timeout_secs: i64,
    ) -> Result<Vec<PendingMessage>> {
        let s = self.clone();
        blocking(move || s.claim_pending_messages(limit, visibility_timeout_secs)).await
    }

    async fn complete_message(&self, id: i64) -> Result<()> {
        let s = self.clone();
        blocking(move || s.complete_message(id)).await
    }

    async fn fail_message(&self, id: i64, increment_retry: bool) -> Result<()> {
        let s = self.clone();
        blocking(move || s.fail_message(id, increment_retry)).await
    }

    async fn get_pending_count(&self) -> Result<usize> {
        let s = self.clone();
        blocking(move || s.get_pending_count()).await
    }

    async fn release_stale_messages(&self, visibility_timeout_secs: i64) -> Result<usize> {
        let s = self.clone();
        blocking(move || s.release_stale_messages(visibility_timeout_secs)).await
    }

    async fn get_failed_messages(&self, limit: usize) -> Result<Vec<PendingMessage>> {
        let s = self.clone();
        blocking(move || s.get_failed_messages(limit)).await
    }

    async fn get_all_pending_messages(&self, limit: usize) -> Result<Vec<PendingMessage>> {
        let s = self.clone();
        blocking(move || s.get_all_pending_messages(limit)).await
    }

    async fn get_queue_stats(&self) -> Result<QueueStats> {
        let s = self.clone();
        blocking(move || s.get_queue_stats()).await
    }

    async fn clear_failed_messages(&self) -> Result<usize> {
        let s = self.clone();
        blocking(move || s.clear_failed_messages()).await
    }

    async fn retry_failed_messages(&self) -> Result<usize> {
        let s = self.clone();
        blocking(move || s.retry_failed_messages()).await
    }

    async fn clear_all_pending_messages(&self) -> Result<usize> {
        let s = self.clone();
        blocking(move || s.clear_all_pending_messages()).await
    }
}

// ── PromptStore ──────────────────────────────────────────────────

#[async_trait]
impl PromptStore for Storage {
    async fn save_user_prompt(&self, prompt: &UserPrompt) -> Result<()> {
        let s = self.clone();
        let prompt = prompt.clone();
        blocking(move || s.save_user_prompt(&prompt)).await
    }

    async fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>> {
        let s = self.clone();
        let project = project.map(ToOwned::to_owned);
        blocking(move || s.get_prompts_paginated(offset, limit, project.as_deref())).await
    }

    async fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>> {
        let s = self.clone();
        let id = id.to_owned();
        blocking(move || s.get_prompt_by_id(&id)).await
    }

    async fn search_prompts(&self, query: &str, limit: usize) -> Result<Vec<UserPrompt>> {
        let s = self.clone();
        let query = query.to_owned();
        blocking(move || s.search_prompts(&query, limit)).await
    }
}

// ── StatsStore ───────────────────────────────────────────────────

#[async_trait]
impl StatsStore for Storage {
    async fn get_stats(&self) -> Result<StorageStats> {
        let s = self.clone();
        blocking(move || s.get_stats()).await
    }

    async fn get_all_projects(&self) -> Result<Vec<String>> {
        let s = self.clone();
        blocking(move || s.get_all_projects()).await
    }

    async fn get_observations_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<Observation>> {
        let s = self.clone();
        let project = project.map(ToOwned::to_owned);
        blocking(move || s.get_observations_paginated(offset, limit, project.as_deref())).await
    }
}

// ── SearchStore ──────────────────────────────────────────────────

#[async_trait]
impl SearchStore for Storage {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let s = self.clone();
        let query = query.to_owned();
        blocking(move || s.search(&query, limit)).await
    }

    async fn hybrid_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let s = self.clone();
        let query = query.to_owned();
        blocking(move || s.hybrid_search(&query, limit)).await
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
        let s = self.clone();
        let query = query.map(ToOwned::to_owned);
        let project = project.map(ToOwned::to_owned);
        let obs_type = obs_type.map(ToOwned::to_owned);
        let from = from.map(ToOwned::to_owned);
        let to = to.map(ToOwned::to_owned);
        blocking(move || {
            s.search_with_filters(
                query.as_deref(),
                project.as_deref(),
                obs_type.as_deref(),
                from.as_deref(),
                to.as_deref(),
                limit,
            )
        })
        .await
    }

    async fn get_timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let s = self.clone();
        let from = from.map(ToOwned::to_owned);
        let to = to.map(ToOwned::to_owned);
        blocking(move || s.get_timeline(from.as_deref(), to.as_deref(), limit)).await
    }

    async fn semantic_search(&self, query_vec: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        let s = self.clone();
        let query_vec = query_vec.to_vec();
        blocking(move || s.semantic_search(&query_vec, limit)).await
    }

    async fn hybrid_search_v2(
        &self,
        query: &str,
        query_vec: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let s = self.clone();
        let query = query.to_owned();
        let query_vec = query_vec.to_vec();
        blocking(move || s.hybrid_search_v2(&query, &query_vec, limit)).await
    }
}

// ── EmbeddingStore ───────────────────────────────────────────────

#[async_trait]
impl EmbeddingStore for Storage {
    async fn store_embedding(&self, observation_id: &str, embedding: &[f32]) -> Result<()> {
        let s = self.clone();
        let observation_id = observation_id.to_owned();
        let embedding = embedding.to_vec();
        blocking(move || s.store_embedding(&observation_id, &embedding)).await
    }

    async fn get_observations_without_embeddings(&self, limit: usize) -> Result<Vec<Observation>> {
        let s = self.clone();
        blocking(move || s.get_observations_without_embeddings(limit)).await
    }

    async fn clear_embeddings(&self) -> Result<()> {
        let s = self.clone();
        blocking(move || s.clear_embeddings()).await
    }

    async fn find_similar(
        &self,
        embedding: &[f32],
        threshold: f32,
    ) -> Result<Option<SimilarMatch>> {
        let s = self.clone();
        let embedding = embedding.to_vec();
        blocking(move || s.find_similar(&embedding, threshold)).await
    }
}
