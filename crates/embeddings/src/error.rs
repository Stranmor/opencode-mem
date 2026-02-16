//! Typed error enum for the embeddings crate.

use thiserror::Error;

/// Errors from embedding generation operations.
#[derive(Debug, Error)]
pub enum EmbeddingError {
    #[error("embedding model initialization failed: {0}")]
    ModelInit(String),
    #[error("embedding mutex lock poisoned")]
    LockPoisoned,
    #[error("embedding generation returned empty result")]
    EmptyResult,
    #[error("embedding generation failed: {0}")]
    Generation(String),
}
