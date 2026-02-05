use std::io;
use std::result::Result as StdResult;

use thiserror::Error;

/// Errors that can occur in opencode-mem
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum MemoryError {
    /// Storage layer error (file system, permissions).
    #[error("Storage error: {0}")]
    Storage(String),

    /// Database operation error (`SQLite`, `PostgreSQL`).
    #[error("Database error: {0}")]
    Database(String),

    /// JSON serialization/deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// LLM API call error (network, rate limit, invalid response).
    #[error("LLM API error: {0}")]
    LlmApi(String),

    /// Embedding generation error.
    #[error("Embedding error: {0}")]
    Embedding(String),

    /// Search operation error.
    #[error("Search error: {0}")]
    Search(String),

    /// Requested resource not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// Invalid input provided by caller.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// IO operation error.
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

/// Result type alias for opencode-mem operations.
pub type Result<T> = StdResult<T, MemoryError>;
