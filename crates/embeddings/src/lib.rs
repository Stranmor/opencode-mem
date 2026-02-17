//! Embedding generation for semantic search using fastembed-rs
//!
//! Provides local embedding generation using `BGE-M3` model (1024 dimensions, 100+ languages).

#![allow(clippy::missing_docs_in_private_items, reason = "Internal crate")]
#![allow(clippy::implicit_return, reason = "Implicit return is idiomatic Rust")]
#![allow(clippy::question_mark_used, reason = "? operator is idiomatic Rust")]

pub mod error;

use error::EmbeddingError;
use fastembed::{InitOptions, TextEmbedding};
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::sync::Mutex;

/// Embedding dimension for `BGE-M3` model (re-exported from core)
pub use opencode_mem_core::EMBEDDING_DIMENSION;

/// Trait for embedding providers
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for a single text
    ///
    /// # Errors
    /// Returns error if embedding generation fails
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;

    /// Generate embeddings for multiple texts
    ///
    /// # Errors
    /// Returns error if embedding generation fails
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError>;

    /// Get the embedding dimension
    fn dimension(&self) -> usize;
}

/// Embedding service using fastembed with `BGE-M3` multilingual model
pub struct EmbeddingService {
    model: Mutex<TextEmbedding>,
}

impl Debug for EmbeddingService {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("EmbeddingService").field("model", &"<TextEmbedding>").finish()
    }
}

impl EmbeddingService {
    /// Create a new embedding service
    ///
    /// Downloads the model on first use if not cached.
    ///
    /// # Errors
    /// Returns error if model initialization fails
    pub fn new() -> Result<Self, EmbeddingError> {
        let options =
            InitOptions::new(fastembed::EmbeddingModel::BGEM3).with_show_download_progress(true);

        let model = TextEmbedding::try_new(options)
            .map_err(|e| EmbeddingError::ModelInit(e.to_string()))?;

        tracing::info!(
            model = "BGE-M3",
            dimension = EMBEDDING_DIMENSION,
            "Embedding service initialized"
        );

        Ok(Self { model: Mutex::new(model) })
    }
}

impl EmbeddingProvider for EmbeddingService {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let embeddings = self
            .model
            .lock()
            .map_err(|_| EmbeddingError::LockPoisoned)?
            .embed(vec![text], None)
            .map_err(|e| EmbeddingError::Generation(e.to_string()))?;
        embeddings.into_iter().next().ok_or(EmbeddingError::EmptyResult)
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let embeddings = self
            .model
            .lock()
            .map_err(|_| EmbeddingError::LockPoisoned)?
            .embed(texts, None)
            .map_err(|e| EmbeddingError::Generation(e.to_string()))?;
        Ok(embeddings)
    }

    fn dimension(&self) -> usize {
        EMBEDDING_DIMENSION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[expect(clippy::expect_used, reason = "test code - panic on failure is acceptable")]
    fn test_embedding_dimension() {
        let service = EmbeddingService::new().expect("Failed to create service");
        assert_eq!(service.dimension(), EMBEDDING_DIMENSION);
    }
}
