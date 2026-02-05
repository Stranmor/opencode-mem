//! Embedding generation for semantic search using fastembed-rs
//!
//! Provides local embedding generation using `AllMiniLML6V2` model (384 dimensions).

#![allow(clippy::missing_docs_in_private_items, reason = "Internal crate")]
#![allow(clippy::implicit_return, reason = "Implicit return is idiomatic Rust")]
#![allow(clippy::question_mark_used, reason = "? operator is idiomatic Rust")]

use anyhow::Result;
use fastembed::{InitOptions, TextEmbedding};
use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::sync::Mutex;

/// Embedding dimension for `AllMiniLML6V2` model
pub const EMBEDDING_DIMENSION: usize = 384;

/// Trait for embedding providers
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for a single text
    ///
    /// # Errors
    /// Returns error if embedding generation fails
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for multiple texts
    ///
    /// # Errors
    /// Returns error if embedding generation fails
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// Get the embedding dimension
    fn dimension(&self) -> usize;
}

/// Embedding service using fastembed with `AllMiniLML6V2` model
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
    pub fn new() -> Result<Self> {
        let options = InitOptions::new(fastembed::EmbeddingModel::AllMiniLML6V2)
            .with_show_download_progress(true);

        let model = TextEmbedding::try_new(options)?;

        tracing::info!(
            model = "AllMiniLML6V2",
            dimension = 384i32,
            "Embedding service initialized"
        );

        Ok(Self { model: Mutex::new(model) })
    }
}

impl EmbeddingProvider for EmbeddingService {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self
            .model
            .lock()
            .map_err(|err| anyhow::anyhow!("Lock poisoned: {err}"))?
            .embed(vec![text], None)?;
        embeddings.into_iter().next().ok_or_else(|| anyhow::anyhow!("No embedding returned"))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let embeddings = self
            .model
            .lock()
            .map_err(|err| anyhow::anyhow!("Lock poisoned: {err}"))?
            .embed(texts, None)?;
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
        assert_eq!(service.dimension(), 384);
    }
}
