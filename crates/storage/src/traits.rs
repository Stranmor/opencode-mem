//! Storage backend trait abstraction
//!
//! Defines async domain traits for storage operations, enabling
//! PostgreSQL-primary with SQLite-fallback via enum dispatch.

use anyhow::Result;
use async_trait::async_trait;
use opencode_mem_core::{
    GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType, Observation,
    SearchResult, Session, SessionStatus, SessionSummary, SimilarMatch, UserPrompt,
};

use crate::pending_queue::{PaginatedResult, PendingMessage, QueueStats, StorageStats};

/// CRUD operations on observations.
#[async_trait]
pub trait ObservationStore: Send + Sync {
    /// Save observation. Returns `true` if inserted, `false` on duplicate.
    async fn save_observation(&self, obs: &Observation) -> Result<bool>;

    /// Get observation by ID.
    async fn get_by_id(&self, id: &str) -> Result<Option<Observation>>;

    /// Get recent observations.
    async fn get_recent(&self, limit: usize) -> Result<Vec<Observation>>;

    /// Get all observations for a session.
    async fn get_session_observations(&self, session_id: &str) -> Result<Vec<Observation>>;

    /// Get observations by a list of IDs.
    async fn get_observations_by_ids(&self, ids: &[String]) -> Result<Vec<Observation>>;

    /// Get observations for a project.
    async fn get_context_for_project(
        &self,
        project: &str,
        limit: usize,
    ) -> Result<Vec<Observation>>;

    /// Count observations in a session.
    async fn get_session_observation_count(&self, session_id: &str) -> Result<usize>;

    /// Search observations by file path.
    async fn search_by_file(&self, file_path: &str, limit: usize) -> Result<Vec<SearchResult>>;

    /// Merge a newer observation into an existing one (semantic dedup).
    async fn merge_into_existing(&self, existing_id: &str, newer: &Observation) -> Result<()>;
}

/// Session lifecycle operations.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Save or replace a session.
    async fn save_session(&self, session: &Session) -> Result<()>;

    /// Get session by ID.
    async fn get_session(&self, id: &str) -> Result<Option<Session>>;

    /// Get session by content session ID.
    async fn get_session_by_content_id(&self, content_session_id: &str) -> Result<Option<Session>>;

    /// Update session status.
    async fn update_session_status(&self, id: &str, status: SessionStatus) -> Result<()>;

    /// Delete session. Returns `true` if a row was deleted.
    async fn delete_session(&self, session_id: &str) -> Result<bool>;

    /// Close sessions that have been active longer than `max_age_hours`.
    async fn close_stale_sessions(&self, max_age_hours: i64) -> Result<usize>;
}

/// Knowledge base operations.
#[async_trait]
pub trait KnowledgeStore: Send + Sync {
    /// Save or update a knowledge entry (upserts by title).
    async fn save_knowledge(&self, input: KnowledgeInput) -> Result<GlobalKnowledge>;

    /// Get knowledge entry by ID.
    async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>>;

    /// Delete knowledge entry by ID. Returns `true` if deleted.
    async fn delete_knowledge(&self, id: &str) -> Result<bool>;

    /// Full-text search over knowledge.
    async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>>;

    /// List knowledge entries, optionally filtered by type.
    async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>>;

    /// Increment usage count and bump confidence.
    async fn update_knowledge_usage(&self, id: &str) -> Result<()>;
}

/// Session summary operations.
#[async_trait]
pub trait SummaryStore: Send + Sync {
    /// Save session summary.
    async fn save_summary(&self, summary: &SessionSummary) -> Result<()>;

    /// Get session summary by session ID.
    async fn get_session_summary(&self, session_id: &str) -> Result<Option<SessionSummary>>;

    /// Update session status and optionally save summary text.
    async fn update_session_status_with_summary(
        &self,
        session_id: &str,
        status: SessionStatus,
        summary: Option<&str>,
    ) -> Result<()>;

    /// Get summaries with pagination.
    async fn get_summaries_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<SessionSummary>>;

    /// Full-text search over session summaries.
    async fn search_sessions(&self, query: &str, limit: usize) -> Result<Vec<SessionSummary>>;
}

/// Pending message queue operations.
#[async_trait]
pub trait PendingQueueStore: Send + Sync {
    /// Queue a message for processing. Returns the new message ID.
    async fn queue_message(
        &self,
        session_id: &str,
        tool_name: Option<&str>,
        tool_input: Option<&str>,
        tool_response: Option<&str>,
        project: Option<&str>,
    ) -> Result<i64>;

    /// Claim pending messages for processing.
    async fn claim_pending_messages(
        &self,
        limit: usize,
        visibility_timeout_secs: i64,
    ) -> Result<Vec<PendingMessage>>;

    /// Delete message after successful processing.
    async fn complete_message(&self, id: i64) -> Result<()>;

    /// Mark message as failed.
    async fn fail_message(&self, id: i64, increment_retry: bool) -> Result<()>;

    /// Get count of pending messages.
    async fn get_pending_count(&self) -> Result<usize>;

    /// Release stale processing messages back to pending.
    async fn release_stale_messages(&self, visibility_timeout_secs: i64) -> Result<usize>;

    /// Get failed messages.
    async fn get_failed_messages(&self, limit: usize) -> Result<Vec<PendingMessage>>;

    /// Get all pending messages.
    async fn get_all_pending_messages(&self, limit: usize) -> Result<Vec<PendingMessage>>;

    /// Get queue statistics.
    async fn get_queue_stats(&self) -> Result<QueueStats>;

    /// Clear all failed messages.
    async fn clear_failed_messages(&self) -> Result<usize>;

    /// Reset failed messages back to pending for retry.
    async fn retry_failed_messages(&self) -> Result<usize>;

    /// Clear all pending messages.
    async fn clear_all_pending_messages(&self) -> Result<usize>;
}

/// User prompt operations.
#[async_trait]
pub trait PromptStore: Send + Sync {
    /// Save user prompt.
    async fn save_user_prompt(&self, prompt: &UserPrompt) -> Result<()>;

    /// Get prompts with pagination.
    async fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>>;

    /// Get prompt by ID.
    async fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>>;

    /// Search prompts by text.
    async fn search_prompts(&self, query: &str, limit: usize) -> Result<Vec<UserPrompt>>;
}

/// Aggregate statistics.
#[async_trait]
pub trait StatsStore: Send + Sync {
    /// Get storage statistics.
    async fn get_stats(&self) -> Result<StorageStats>;

    /// Get all distinct projects.
    async fn get_all_projects(&self) -> Result<Vec<String>>;

    /// Get observations with pagination.
    async fn get_observations_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<Observation>>;
}

/// Text and hybrid search operations.
#[async_trait]
pub trait SearchStore: Send + Sync {
    /// FTS5 full-text search.
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;

    /// Hybrid search combining FTS5 and keyword matching.
    async fn hybrid_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;

    /// Search with optional filters for project, type, and date range.
    async fn search_with_filters(
        &self,
        query: Option<&str>,
        project: Option<&str>,
        obs_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>>;

    /// Get observations within a time range.
    async fn get_timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>>;

    /// Vector similarity search.
    async fn semantic_search(&self, query_vec: &[f32], limit: usize) -> Result<Vec<SearchResult>>;

    /// Hybrid search: FTS5 BM25 (50%) + vector cosine similarity (50%).
    async fn hybrid_search_v2(
        &self,
        query: &str,
        query_vec: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>>;
}

/// Embedding storage operations.
#[async_trait]
pub trait EmbeddingStore: Send + Sync {
    /// Store an embedding vector for an observation.
    async fn store_embedding(&self, observation_id: &str, embedding: &[f32]) -> Result<()>;

    /// Get observations that don't have embeddings yet.
    async fn get_observations_without_embeddings(&self, limit: usize) -> Result<Vec<Observation>>;

    /// Drop and recreate the embedding index, forcing re-embedding of all observations.
    async fn clear_embeddings(&self) -> Result<()>;

    /// Find the most similar existing observation by cosine similarity.
    async fn find_similar(&self, embedding: &[f32], threshold: f32)
        -> Result<Option<SimilarMatch>>;

    /// Find top-N similar observations above a similarity threshold.
    ///
    /// Returns matches ordered by similarity descending, up to `limit` results.
    /// Used for providing context to the LLM (lower threshold than dedup).
    async fn find_similar_many(
        &self,
        embedding: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Result<Vec<SimilarMatch>>;
}
