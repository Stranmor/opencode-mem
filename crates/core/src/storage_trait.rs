//! Storage backend abstraction trait
//!
//! Provides a common interface for `SQLite` (`Storage`) and `PostgreSQL` (`InfiniteMemory`) backends.
//! Enables mocking, testing, and backend-agnostic code.

use crate::{Observation, SearchResult, Session};
use anyhow::Result;
use async_trait::async_trait;

/// Common storage backend interface for observations and sessions.
///
/// Both `SQLite`-based `Storage` and `PostgreSQL`-based `InfiniteMemory` implement this trait.
/// The trait is async to accommodate `PostgreSQL`'s async nature; `SQLite` implementations
/// use `spawn_blocking` internally.
#[async_trait]
pub trait StorageBackend: Send + Sync {
    // ─────────────────────────────────────────────────────────────────────────────
    // Observations
    // ─────────────────────────────────────────────────────────────────────────────

    /// Save an observation to storage.
    async fn save_observation(&self, obs: &Observation) -> Result<()>;

    /// Get an observation by ID.
    async fn get_observation(&self, id: &str) -> Result<Option<Observation>>;

    /// Search observations by query string.
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;

    /// Get recent observations.
    async fn get_recent(&self, limit: usize) -> Result<Vec<SearchResult>>;

    // ─────────────────────────────────────────────────────────────────────────────
    // Sessions
    // ─────────────────────────────────────────────────────────────────────────────

    /// Save a session to storage.
    async fn save_session(&self, session: &Session) -> Result<()>;

    /// Get a session by ID.
    async fn get_session(&self, id: &str) -> Result<Option<Session>>;
}
