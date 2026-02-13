//! Semantic deduplication types and helpers for observations.

use super::Observation;

/// A match found during semantic deduplication.
#[derive(Debug, Clone)]
pub struct SimilarMatch {
    /// ID of the existing similar observation.
    pub observation_id: String,
    /// Cosine similarity score (0.0–1.0).
    pub similarity: f32,
    /// Title of the existing observation (for logging).
    pub title: String,
}

/// Builds a single text string from an observation for embedding generation.
///
/// Concatenates title, narrative, and facts into a space-separated string
/// suitable for vector embedding.
#[must_use]
pub fn observation_embedding_text(obs: &Observation) -> String {
    format!("{} {} {}", obs.title, obs.narrative.as_deref().unwrap_or(""), obs.facts.join(" "))
}
