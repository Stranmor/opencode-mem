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
