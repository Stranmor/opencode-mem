//! Typed error enum for the service layer.
//!
//! Unifies storage, LLM, and embedding failures into a single error type,
//! enabling callers to match on specific failure modes instead of downcasting
//! opaque `anyhow::Error` boxes.

use opencode_mem_embeddings::error::EmbeddingError;
use opencode_mem_llm::LlmError;
use opencode_mem_storage::StorageError;
use thiserror::Error;

/// Service-layer error unifying storage, LLM, and embedding failures.
#[derive(Debug, Error)]
pub enum ServiceError {
    /// Storage operation failed (DB, not found, duplicate, etc.).
    #[error("storage: {0}")]
    Storage(#[from] StorageError),

    /// LLM API call failed.
    #[error("llm: {0}")]
    Llm(#[from] LlmError),

    /// Embedding generation failed.
    #[error("embedding: {0}")]
    Embedding(#[from] EmbeddingError),

    /// Caller provided invalid input (empty text, malformed data).
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Required backend (LLM, embeddings, infinite memory) is not configured.
    #[error("not configured: {0}")]
    NotConfigured(String),

    /// External process execution failed (e.g., `opencode export`).
    #[error("external command: {0}")]
    ExternalCommand(String),

    /// Serialization/deserialization failed in the service layer.
    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Search operation failed (wraps anyhow from search crate).
    #[error("search: {0}")]
    Search(#[source] anyhow::Error),

    /// General system or unclassified errors.
    #[error("system: {0}")]
    System(#[from] anyhow::Error),
}

impl ServiceError {
    /// Whether this error is likely transient (worth retrying).
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Storage(e) => e.is_transient(),
            Self::Llm(e) => e.is_transient(),
            Self::Search(e) => {
                e.downcast_ref::<StorageError>().is_some_and(StorageError::is_transient)
            },
            _ => false,
        }
    }

    /// Whether this error represents a not-found condition.
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::Storage(StorageError::NotFound { .. }))
    }

    /// Whether this error is a unique-constraint violation.
    pub fn is_duplicate(&self) -> bool {
        matches!(self, Self::Storage(e) if e.is_duplicate())
    }

    /// Whether the database is completely unavailable.
    ///
    /// Checks both explicit `Unavailable` and connection-level failures.
    /// Also checks `Search(anyhow)` by downcasting and inspecting error message
    /// for common connection failure patterns.
    pub fn is_db_unavailable(&self) -> bool {
        match self {
            Self::Storage(e) => e.is_unavailable(),
            Self::Search(e) => {
                if e.downcast_ref::<StorageError>().is_some_and(StorageError::is_unavailable) {
                    return true;
                }
                // anyhow chain may not contain StorageError directly —
                // check the error string for connection failure patterns.
                let msg = format!("{e:?}");
                msg.contains("No route to host")
                    || msg.contains("connection refused")
                    || msg.contains("Connection reset")
                    || msg.contains("PoolTimedOut")
                    || msg.contains("PoolClosed")
                    || msg.contains("WorkerCrashed")
            },
            Self::System(e) => {
                let msg = format!("{e:?}");
                msg.contains("No route to host")
                    || msg.contains("connection refused")
                    || msg.contains("PoolTimedOut")
                    || msg.contains("PoolClosed")
            },
            _ => false,
        }
    }
}

impl From<std::io::Error> for ServiceError {
    fn from(err: std::io::Error) -> Self {
        Self::ExternalCommand(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for ServiceError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        Self::InvalidInput(format!("invalid UTF-8: {err}"))
    }
}
