use async_trait::async_trait;
use opencode_mem_core::{Session, SessionStatus, SessionSummary};

use crate::error::StorageError;
use crate::pending_queue::PaginatedResult;

/// Session lifecycle operations.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Save or replace a session.
    async fn save_session(&self, session: &Session) -> Result<(), StorageError>;

    /// Get session by ID.
    async fn get_session(&self, id: &str) -> Result<Option<Session>, StorageError>;

    /// Get session by content session ID.
    async fn get_session_by_content_id(
        &self,
        content_session_id: &str,
    ) -> Result<Option<Session>, StorageError>;

    /// Update session status.
    async fn update_session_status(
        &self,
        id: &str,
        status: SessionStatus,
    ) -> Result<(), StorageError>;

    /// Delete session. Returns `true` if a row was deleted.
    async fn delete_session(&self, session_id: &str) -> Result<bool, StorageError>;

    /// Close sessions that have been active longer than `max_age_hours`.
    async fn close_stale_sessions(&self, max_age_hours: i64) -> Result<usize, StorageError>;
}

/// Session summary operations.
#[async_trait]
pub trait SummaryStore: Send + Sync {
    /// Save session summary.
    async fn save_summary(&self, summary: &SessionSummary) -> Result<(), StorageError>;

    /// Get session summary by session ID.
    async fn get_session_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionSummary>, StorageError>;

    /// Update session status and optionally save summary text.
    async fn update_session_status_with_summary(
        &self,
        session_id: &str,
        status: SessionStatus,
        summary: Option<&str>,
    ) -> Result<(), StorageError>;

    /// Get summaries with pagination.
    async fn get_summaries_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<SessionSummary>, StorageError>;

    /// Full-text search over session summaries.
    async fn search_sessions(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SessionSummary>, StorageError>;
}
