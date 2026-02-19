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
use std::sync::{Mutex, Once};

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

/// Ensures ORT global thread pool is configured exactly once
static ORT_INIT: Once = Once::new();

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
        let thread_count = Self::get_thread_count();

        ORT_INIT.call_once(|| {
            // OMP_NUM_THREADS is the ONLY reliable way to limit threads when ONNX Runtime
            // is built with OpenMP (Microsoft's prebuilt binaries). Per-session
            // `with_intra_threads` and global pool options have no effect in OpenMP builds.
            // Safe here: called once via Once, before any ONNX/OpenMP initialization.
            // TODO(edition-2024): wrap in unsafe {} when migrating to Rust edition 2024
            if std::env::var("OMP_NUM_THREADS").is_err() {
                std::env::set_var("OMP_NUM_THREADS", thread_count.to_string());
            }

            let pool_opts = ort::environment::GlobalThreadPoolOptions::default()
                .with_intra_threads(thread_count)
                .and_then(|opts| opts.with_spin_control(false));

            match pool_opts {
                Ok(opts) => {
                    let applied = ort::init().with_global_thread_pool(opts).commit();
                    if applied {
                        tracing::info!(threads = thread_count, "ORT global thread pool configured");
                    } else {
                        tracing::debug!("ORT environment already configured, thread pool settings skipped");
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to configure ORT global thread pool, using defaults");
                }
            }
        });

        #[allow(unused_mut, reason = "mut needed when cuda feature is enabled")]
        let mut options =
            InitOptions::new(fastembed::EmbeddingModel::BGEM3).with_show_download_progress(true);

        #[cfg(feature = "cuda")]
        {
            options = options.with_execution_providers(vec![
                ort::execution_providers::CUDAExecutionProvider::default().build(),
            ]);
            tracing::info!("CUDA execution provider requested for embeddings");
        }

        let model = TextEmbedding::try_new(options)
            .map_err(|e| EmbeddingError::ModelInit(e.to_string()))?;

        tracing::info!(
            model = "BGE-M3",
            dimension = EMBEDDING_DIMENSION,
            gpu = cfg!(feature = "cuda"),
            threads = thread_count,
            "Embedding service initialized"
        );

        Ok(Self { model: Mutex::new(model) })
    }

    fn get_thread_count() -> usize {
        let max_threads = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1);
        let default_threads = max_threads.saturating_sub(1).max(1);
        let configured =
            opencode_mem_core::env_parse_with_default("OPENCODE_MEM_EMBEDDING_THREADS", 0_usize);
        if configured == 0 { default_threads } else { configured.clamp(1, max_threads) }
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
