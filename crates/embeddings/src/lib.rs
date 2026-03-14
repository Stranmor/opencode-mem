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
use std::sync::{Mutex, Once, OnceLock};

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

/// Embedding service using fastembed with `BGE-M3` multilingual model.
///
/// `Mutex` is required because `TextEmbedding::embed()` takes `&mut self`.
/// The underlying `ort::Session` is `Send + Sync`, but fastembed's API
/// requires mutable access for tokenization + inference pipeline state.
pub struct EmbeddingService {
    model: Mutex<TextEmbedding>,
}

/// Ensures ORT global thread pool is configured exactly once
static ORT_INIT: Once = Once::new();

/// Configures ORT global thread pool (idempotent via `Once`).
fn init_ort(thread_count: usize) {
    ORT_INIT.call_once(|| {
        // OMP_NUM_THREADS is the ONLY reliable way to limit threads when ONNX Runtime
        // is built with OpenMP (Microsoft's prebuilt binaries). Per-session
        // `with_intra_threads` and global pool options have no effect in OpenMP builds.
        // Safe here: called once via Once, before any ONNX/OpenMP initialization.
        // TODO(edition-2024): wrap in unsafe {} when migrating to Rust edition 2024

        let pool_opts = ort::environment::GlobalThreadPoolOptions::default()
            .with_intra_threads(thread_count)
            .and_then(|opts| opts.with_spin_control(false));

        match pool_opts {
            Ok(opts) => {
                let applied = ort::init().with_global_thread_pool(opts).commit();
                if applied {
                    tracing::info!(threads = thread_count, "ORT global thread pool configured");
                } else {
                    tracing::debug!(
                        "ORT environment already configured, thread pool settings skipped"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to configure ORT global thread pool, using defaults");
            }
        }
    });
}

impl Debug for EmbeddingService {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("EmbeddingService")
            .field("model", &"<TextEmbedding>")
            .finish()
    }
}

impl EmbeddingService {
    /// Create a new embedding service
    ///
    /// Downloads the model on first use if not cached.
    /// This is CPU-intensive (~28s) — prefer [`LazyEmbeddingService`] for server startup.
    ///
    /// # Errors
    /// Returns error if model initialization fails
    pub fn new(thread_count: usize) -> Result<Self, EmbeddingError> {
        init_ort(thread_count);

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

        Ok(Self {
            model: Mutex::new(model),
        })
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
        embeddings
            .into_iter()
            .next()
            .ok_or(EmbeddingError::EmptyResult)
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

/// Lazy-loading wrapper around [`EmbeddingService`].
///
/// Defers the expensive ONNX model initialization (~28s) until the first
/// actual embedding call. This allows MCP/HTTP servers to start instantly
/// and respond to handshake within OpenCode's 30-second timeout.
///
/// Thread-safe: the inner `OnceLock` ensures initialization runs exactly once,
/// even under concurrent access from multiple `spawn_blocking` tasks.
pub struct LazyEmbeddingService {
    inner: OnceLock<Result<EmbeddingService, String>>,
    thread_count: usize,
}

impl Debug for LazyEmbeddingService {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let initialized = self
            .inner
            .get()
            .map_or("pending", |r| if r.is_ok() { "ready" } else { "failed" });
        f.debug_struct("LazyEmbeddingService")
            .field("state", &initialized)
            .field("thread_count", &self.thread_count)
            .finish()
    }
}

impl LazyEmbeddingService {
    /// Create a lazy embedding service that defers model loading to first use.
    #[must_use]
    pub fn new(thread_count: usize) -> Self {
        tracing::info!(
            threads = thread_count,
            "Lazy embedding service created (model will load on first use)"
        );
        Self {
            inner: OnceLock::new(),
            thread_count,
        }
    }

    /// Get or initialize the inner service. Returns error if initialization failed.
    fn get_or_init(&self) -> Result<&EmbeddingService, EmbeddingError> {
        let result = self.inner.get_or_init(|| {
            tracing::info!("First embedding request — initializing model (this may take ~30s)...");
            EmbeddingService::new(self.thread_count).map_err(|e| e.to_string())
        });
        match result {
            Ok(svc) => Ok(svc),
            Err(msg) => Err(EmbeddingError::ModelInit(msg.clone())),
        }
    }
}

impl EmbeddingProvider for LazyEmbeddingService {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        self.get_or_init()?.embed(text)
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        self.get_or_init()?.embed_batch(texts)
    }

    fn dimension(&self) -> usize {
        EMBEDDING_DIMENSION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[expect(
        clippy::expect_used,
        reason = "test code - panic on failure is acceptable"
    )]
    fn test_embedding_dimension() {
        let service = EmbeddingService::new(1).expect("Failed to create service");
        assert_eq!(service.dimension(), EMBEDDING_DIMENSION);
    }
}
