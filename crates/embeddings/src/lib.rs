//! Embedding generation for semantic search
//!
//! TODO: Implement local embedding model (all-MiniLM-L6-v2)

use anyhow::Result;

const EMBEDDING_DIMENSION: usize = 384;

pub struct EmbeddingModel;

impl EmbeddingModel {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    pub fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        // TODO: Implement with candle or onnxruntime
        Ok(vec![0.0; EMBEDDING_DIMENSION])
    }

    pub fn dimension(&self) -> usize {
        EMBEDDING_DIMENSION
    }
}
