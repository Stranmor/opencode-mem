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

/// Body-generating macro for async-to-blocking delegation.
///
/// Each argument is annotated with a capture kind:
/// - `@ref arg`      — `.clone()` a `&T`, pass as `&arg`
/// - `@str arg`      — `.to_owned()` a `&str`, pass as `&arg`
/// - `@opt_str arg`  — `.map(ToOwned::to_owned)` an `Option<&str>`, pass as `arg.as_deref()`
/// - `@slice arg`    — `.to_vec()` a `&[T]`, pass as `&arg`
/// - `@val arg`      — move directly (Copy/owned types)
macro_rules! delegate {
    ($self:ident, $method:ident $(, @$kind:ident $arg:ident)*) => {{
        let s = $self.clone();
        $(delegate!(@capture $kind $arg);)*
        blocking(move || s.$method($(delegate!(@pass $kind $arg)),*)).await
    }};
    (@capture ref $arg:ident) => { let $arg = $arg.clone(); };
    (@capture str $arg:ident) => { let $arg = $arg.to_owned(); };
    (@capture opt_str $arg:ident) => { let $arg = $arg.map(ToOwned::to_owned); };
    (@capture slice $arg:ident) => { let $arg = $arg.to_vec(); };
    (@capture val $arg:ident) => { };
    (@pass ref $arg:ident) => { &$arg };
    (@pass str $arg:ident) => { &$arg };
    (@pass opt_str $arg:ident) => { $arg.as_deref() };
    (@pass slice $arg:ident) => { &$arg };
    (@pass val $arg:ident) => { $arg };
}

// ── ObservationStore ─────────────────────────────────────────────

#[async_trait]
impl ObservationStore for Storage {
    async fn save_observation(&self, obs: &Observation) -> Result<bool> {
        delegate!(self, save_observation, @ref obs)
    }
    async fn get_by_id(&self, id: &str) -> Result<Option<Observation>> {
        delegate!(self, get_by_id, @str id)
    }
    async fn get_recent(&self, limit: usize) -> Result<Vec<SearchResult>> {
        delegate!(self, get_recent, @val limit)
    }
    async fn get_session_observations(&self, session_id: &str) -> Result<Vec<Observation>> {
        delegate!(self, get_session_observations, @str session_id)
    }
    async fn get_observations_by_ids(&self, ids: &[String]) -> Result<Vec<Observation>> {
        delegate!(self, get_observations_by_ids, @slice ids)
    }
    async fn get_context_for_project(
        &self,
        project: &str,
        limit: usize,
    ) -> Result<Vec<Observation>> {
        delegate!(self, get_context_for_project, @str project, @val limit)
    }
    async fn get_session_observation_count(&self, session_id: &str) -> Result<usize> {
        delegate!(self, get_session_observation_count, @str session_id)
    }
    async fn search_by_file(&self, file_path: &str, limit: usize) -> Result<Vec<SearchResult>> {
        delegate!(self, search_by_file, @str file_path, @val limit)
    }
    async fn merge_into_existing(&self, existing_id: &str, newer: &Observation) -> Result<()> {
        delegate!(self, merge_into_existing, @str existing_id, @ref newer)
    }
}

// ── SessionStore ─────────────────────────────────────────────────

#[async_trait]
impl SessionStore for Storage {
    async fn save_session(&self, session: &Session) -> Result<()> {
        delegate!(self, save_session, @ref session)
    }
    async fn get_session(&self, id: &str) -> Result<Option<Session>> {
        delegate!(self, get_session, @str id)
    }
    async fn get_session_by_content_id(&self, content_session_id: &str) -> Result<Option<Session>> {
        delegate!(self, get_session_by_content_id, @str content_session_id)
    }
    async fn update_session_status(&self, id: &str, status: SessionStatus) -> Result<()> {
        delegate!(self, update_session_status, @str id, @val status)
    }
    async fn delete_session(&self, session_id: &str) -> Result<bool> {
        delegate!(self, delete_session, @str session_id)
    }
    async fn close_stale_sessions(&self, max_age_hours: i64) -> Result<usize> {
        delegate!(self, close_stale_sessions, @val max_age_hours)
    }
}

// ── KnowledgeStore ───────────────────────────────────────────────

#[async_trait]
impl KnowledgeStore for Storage {
    async fn save_knowledge(&self, input: KnowledgeInput) -> Result<GlobalKnowledge> {
        delegate!(self, save_knowledge, @val input)
    }
    async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>> {
        delegate!(self, get_knowledge, @str id)
    }
    async fn delete_knowledge(&self, id: &str) -> Result<bool> {
        delegate!(self, delete_knowledge, @str id)
    }
    async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>> {
        delegate!(self, search_knowledge, @str query, @val limit)
    }
    async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>> {
        delegate!(self, list_knowledge, @val knowledge_type, @val limit)
    }
    async fn update_knowledge_usage(&self, id: &str) -> Result<()> {
        delegate!(self, update_knowledge_usage, @str id)
    }
}

// ── SummaryStore ─────────────────────────────────────────────────

#[async_trait]
impl SummaryStore for Storage {
    async fn save_summary(&self, summary: &SessionSummary) -> Result<()> {
        delegate!(self, save_summary, @ref summary)
    }
    async fn get_session_summary(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        delegate!(self, get_session_summary, @str session_id)
    }
    async fn update_session_status_with_summary(
        &self,
        session_id: &str,
        status: SessionStatus,
        summary: Option<&str>,
    ) -> Result<()> {
        delegate!(self, update_session_status_with_summary, @str session_id, @val status, @opt_str summary)
    }
    async fn get_summaries_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<SessionSummary>> {
        delegate!(self, get_summaries_paginated, @val offset, @val limit, @opt_str project)
    }
    async fn search_sessions(&self, query: &str, limit: usize) -> Result<Vec<SessionSummary>> {
        delegate!(self, search_sessions, @str query, @val limit)
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
        delegate!(self, queue_message, @str session_id, @opt_str tool_name, @opt_str tool_input, @opt_str tool_response, @opt_str project)
    }
    async fn claim_pending_messages(
        &self,
        limit: usize,
        visibility_timeout_secs: i64,
    ) -> Result<Vec<PendingMessage>> {
        delegate!(self, claim_pending_messages, @val limit, @val visibility_timeout_secs)
    }
    async fn complete_message(&self, id: i64) -> Result<()> {
        delegate!(self, complete_message, @val id)
    }
    async fn fail_message(&self, id: i64, increment_retry: bool) -> Result<()> {
        delegate!(self, fail_message, @val id, @val increment_retry)
    }
    async fn get_pending_count(&self) -> Result<usize> {
        delegate!(self, get_pending_count)
    }
    async fn release_stale_messages(&self, visibility_timeout_secs: i64) -> Result<usize> {
        delegate!(self, release_stale_messages, @val visibility_timeout_secs)
    }
    async fn get_failed_messages(&self, limit: usize) -> Result<Vec<PendingMessage>> {
        delegate!(self, get_failed_messages, @val limit)
    }
    async fn get_all_pending_messages(&self, limit: usize) -> Result<Vec<PendingMessage>> {
        delegate!(self, get_all_pending_messages, @val limit)
    }
    async fn get_queue_stats(&self) -> Result<QueueStats> {
        delegate!(self, get_queue_stats)
    }
    async fn clear_failed_messages(&self) -> Result<usize> {
        delegate!(self, clear_failed_messages)
    }
    async fn retry_failed_messages(&self) -> Result<usize> {
        delegate!(self, retry_failed_messages)
    }
    async fn clear_all_pending_messages(&self) -> Result<usize> {
        delegate!(self, clear_all_pending_messages)
    }
}

// ── PromptStore ──────────────────────────────────────────────────

#[async_trait]
impl PromptStore for Storage {
    async fn save_user_prompt(&self, prompt: &UserPrompt) -> Result<()> {
        delegate!(self, save_user_prompt, @ref prompt)
    }
    async fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>> {
        delegate!(self, get_prompts_paginated, @val offset, @val limit, @opt_str project)
    }
    async fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>> {
        delegate!(self, get_prompt_by_id, @str id)
    }
    async fn search_prompts(&self, query: &str, limit: usize) -> Result<Vec<UserPrompt>> {
        delegate!(self, search_prompts, @str query, @val limit)
    }
}

// ── StatsStore ───────────────────────────────────────────────────

#[async_trait]
impl StatsStore for Storage {
    async fn get_stats(&self) -> Result<StorageStats> {
        delegate!(self, get_stats)
    }
    async fn get_all_projects(&self) -> Result<Vec<String>> {
        delegate!(self, get_all_projects)
    }
    async fn get_observations_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<Observation>> {
        delegate!(self, get_observations_paginated, @val offset, @val limit, @opt_str project)
    }
}

// ── SearchStore ──────────────────────────────────────────────────

#[async_trait]
impl SearchStore for Storage {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        delegate!(self, search, @str query, @val limit)
    }
    async fn hybrid_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        delegate!(self, hybrid_search, @str query, @val limit)
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
        delegate!(self, search_with_filters, @opt_str query, @opt_str project, @opt_str obs_type, @opt_str from, @opt_str to, @val limit)
    }
    async fn get_timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        delegate!(self, get_timeline, @opt_str from, @opt_str to, @val limit)
    }
    async fn semantic_search(&self, query_vec: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        delegate!(self, semantic_search, @slice query_vec, @val limit)
    }
    async fn hybrid_search_v2(
        &self,
        query: &str,
        query_vec: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        delegate!(self, hybrid_search_v2, @str query, @slice query_vec, @val limit)
    }
}

// ── EmbeddingStore ───────────────────────────────────────────────

#[async_trait]
impl EmbeddingStore for Storage {
    async fn store_embedding(&self, observation_id: &str, embedding: &[f32]) -> Result<()> {
        delegate!(self, store_embedding, @str observation_id, @slice embedding)
    }
    async fn get_observations_without_embeddings(&self, limit: usize) -> Result<Vec<Observation>> {
        delegate!(self, get_observations_without_embeddings, @val limit)
    }
    async fn clear_embeddings(&self) -> Result<()> {
        delegate!(self, clear_embeddings)
    }
    async fn find_similar(
        &self,
        embedding: &[f32],
        threshold: f32,
    ) -> Result<Option<SimilarMatch>> {
        delegate!(self, find_similar, @slice embedding, @val threshold)
    }
}
