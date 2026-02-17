//! Semantic deduplication types and helpers for observations.

use std::collections::HashSet;

use super::{Concept, Observation};

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

/// Merges two string slices, removing duplicates while preserving order.
/// Items from `existing` appear first, then unique items from `newer`.
#[must_use]
pub fn union_dedup(existing: &[String], newer: &[String]) -> Vec<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut result = Vec::with_capacity(existing.len().saturating_add(newer.len()));
    for item in existing.iter().chain(newer.iter()) {
        if seen.insert(item.as_str()) {
            result.push(item.clone());
        }
    }
    result
}

/// Merges two `Concept` slices, removing duplicates while preserving order.
/// Uses `as_str()` representation for equality comparison since `Concept`
/// does not implement `Hash`.
#[must_use]
pub fn union_dedup_concepts(existing: &[Concept], newer: &[Concept]) -> Vec<Concept> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut result = Vec::with_capacity(existing.len().saturating_add(newer.len()));
    for item in existing.iter().chain(newer.iter()) {
        if seen.insert(item.as_str()) {
            result.push(*item);
        }
    }
    result
}

/// Returns `true` if every element in the vector is `0.0`.
/// A zero vector produces NaN in cosine distance, poisoning similarity results.
#[must_use]
pub fn is_zero_vector(v: &[f32]) -> bool {
    v.iter().all(|f| *f == 0.0)
}

/// Cosine similarity between two vectors (0.0–1.0 for non-negative embeddings).
/// Returns 0.0 if either vector is empty or zero-length.
#[must_use]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0_f64;
    let mut norm_a = 0.0_f64;
    let mut norm_b = 0.0_f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let xd = f64::from(*x);
        let yd = f64::from(*y);
        dot += xd * yd;
        norm_a += xd * xd;
        norm_b += yd * yd;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        return 0.0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "cosine similarity is bounded [-1,1], safe f64→f32"
    )]
    let result = (dot / denom) as f32;
    result
}
