//! Embedding generation for semantic search
//!
//! TODO: Implement local embedding model (all-MiniLM-L6-v2)

pub struct EmbeddingModel;

impl EmbeddingModel {
    pub fn new() -> Self {
        Self
    }

    pub fn embed(&self, _text: &str) -> Vec<f32> {
        // TODO: Implement with candle or onnxruntime
        vec![0.0; 384]
    }
}
