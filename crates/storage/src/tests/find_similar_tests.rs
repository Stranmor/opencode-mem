//! Tests for `find_similar` — vector similarity DB integration tests.

#![allow(clippy::unwrap_used)]

use super::{create_test_observation, create_test_storage};
use opencode_mem_core::SimilarMatch;
use opencode_mem_core::EMBEDDING_DIMENSION;

#[test]
fn test_find_similar_returns_none_when_no_embeddings() {
    let (storage, _dir) = create_test_storage();

    // Empty DB, no embeddings stored → should return None.
    let vec_dim: Vec<f32> = vec![1.0; EMBEDDING_DIMENSION];
    let result = storage.find_similar(&vec_dim, 0.5).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_find_similar_returns_match_above_threshold() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-sim-1", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let norm: f32 = (EMBEDDING_DIMENSION as f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; EMBEDDING_DIMENSION];
    storage.store_embedding("obs-sim-1", &unit_vec).unwrap();

    // Query with the identical vector → cosine similarity = 1.0.
    let result = storage.find_similar(&unit_vec, 0.9).unwrap();
    assert!(result.is_some(), "expected a SimilarMatch for identical vector");

    let m: SimilarMatch = result.unwrap();
    assert_eq!(m.observation_id, "obs-sim-1");
    assert!(m.similarity >= 0.9, "similarity {} should be >= 0.9", m.similarity);
}

#[test]
#[expect(
    clippy::indexing_slicing,
    reason = "test code — indices 0 and 1 are safe for EMBEDDING_DIMENSION-sized vec"
)]
fn test_find_similar_returns_none_below_threshold() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-orth-1", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let mut vec_a = vec![0.0_f32; EMBEDDING_DIMENSION];
    vec_a[0] = 1.0;
    storage.store_embedding("obs-orth-1", &vec_a).unwrap();

    let mut vec_b = vec![0.0_f32; EMBEDDING_DIMENSION];
    vec_b[1] = 1.0;

    // Cosine similarity of orthogonal vectors = 0.0, well below 0.9.
    let result = storage.find_similar(&vec_b, 0.9).unwrap();
    assert!(result.is_none(), "orthogonal vectors should not match at threshold 0.9");
}

#[test]
fn test_find_similar_empty_embedding_returns_none() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-empty-emb", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let norm: f32 = (EMBEDDING_DIMENSION as f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; EMBEDDING_DIMENSION];
    storage.store_embedding("obs-empty-emb", &unit_vec).unwrap();

    // Empty query embedding → early return None (implementation guard).
    let empty: Vec<f32> = vec![];
    let result = storage.find_similar(&empty, 0.0).unwrap();
    assert!(result.is_none(), "empty embedding must return None");
}

#[test]
fn test_find_similar_threshold_zero_matches_anything() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-thresh0", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    // Store any normalized embedding
    let norm: f32 = (EMBEDDING_DIMENSION as f32).sqrt();
    let stored: Vec<f32> = vec![1.0 / norm; EMBEDDING_DIMENSION];
    storage.store_embedding("obs-thresh0", &stored).unwrap();

    let mut query = vec![0.5 / norm; EMBEDDING_DIMENSION];
    if let Some(first) = query.first_mut() {
        *first = 0.9; // skew direction slightly
    }

    let result = storage.find_similar(&query, 0.0).unwrap();
    assert!(result.is_some(), "threshold 0.0 should match any non-orthogonal vector");
}

#[test]
fn test_find_similar_threshold_one_requires_exact_match() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-thresh1", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    // Use axis-aligned unit vector: single 1.0 component, rest 0.0.
    // This avoids f32 accumulation error — dot product is exactly 1.0,
    // norms are exactly 1.0, so cosine similarity = 1.0 precisely.
    let mut unit_vec = vec![0.0_f32; EMBEDDING_DIMENSION];
    if let Some(first) = unit_vec.first_mut() {
        *first = 1.0;
    }
    storage.store_embedding("obs-thresh1", &unit_vec).unwrap();

    // Identical vector with threshold=1.0 should match (similarity=1.0, >= 1.0).
    let result_exact = storage.find_similar(&unit_vec, 1.0).unwrap();
    assert!(result_exact.is_some(), "identical vector at threshold=1.0 must match");

    // Slightly different vector with threshold=1.0 should NOT match.
    let mut slightly_off = unit_vec.clone();
    if let Some(first) = slightly_off.first_mut() {
        *first = 0.99;
    }
    if let Some(second) = slightly_off.get_mut(1) {
        *second = 0.14; // sin(acos(0.99)) ≈ 0.14, keeps it roughly unit-length
    }
    let result_off = storage.find_similar(&slightly_off, 1.0).unwrap();
    assert!(result_off.is_none(), "perturbed vector at threshold=1.0 must NOT match");
}

#[test]
#[expect(
    clippy::indexing_slicing,
    reason = "test code — indices 0..3 are safe for EMBEDDING_DIMENSION-sized vec"
)]
fn test_find_similar_multiple_embeddings_returns_best_match() {
    let (storage, _dir) = create_test_storage();

    // Store 3 observations with different embeddings.
    for i in 0..3_usize {
        let id = format!("obs-multi-{i}");
        let obs = create_test_observation(&id, "proj");
        assert!(storage.save_observation(&obs).unwrap());

        // Each vector points in e_i direction (unit vector along axis i).
        let mut vec = vec![0.0_f32; EMBEDDING_DIMENSION];
        vec[i] = 1.0;
        storage.store_embedding(&id, &vec).unwrap();
    }

    // Query for e_0 direction → should match obs-multi-0 with similarity=1.0
    let mut query = vec![0.0_f32; EMBEDDING_DIMENSION];
    query[0] = 1.0;

    let result = storage.find_similar(&query, 0.5).unwrap();
    assert!(result.is_some(), "must find best match among 3 embeddings");

    let m = result.unwrap();
    assert_eq!(m.observation_id, "obs-multi-0", "must return the closest match, not arbitrary");
    assert!(
        m.similarity > 0.99,
        "identical direction should have similarity ~1.0, got {}",
        m.similarity
    );
}

#[test]
#[expect(
    clippy::indexing_slicing,
    reason = "test code — indices 0 and 1 are safe for EMBEDDING_DIMENSION-sized vec"
)]
fn test_find_similar_exact_threshold_boundary() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-boundary", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    // Use axis-aligned unit vectors to avoid f32 precision issues.
    // stored = [1, 0, 0, ...], query = [0.8, 0.6, 0, ...] (already unit-length).
    // Cosine similarity = dot / (|a|*|b|) = 0.8 / (1.0 * 1.0) = 0.8 exactly,
    // because only one stored component is non-zero, so dot = query[0] * 1.0 = 0.8.
    let mut stored = vec![0.0_f32; EMBEDDING_DIMENSION];
    stored[0] = 1.0;
    storage.store_embedding("obs-boundary", &stored).unwrap();

    let mut query = vec![0.0_f32; EMBEDDING_DIMENSION];
    query[0] = 0.8;
    query[1] = 0.6; // |query| = sqrt(0.64 + 0.36) = 1.0 in exact math

    // Threshold at 0.79 (just below similarity) → should match.
    let result_at = storage.find_similar(&query, 0.79).unwrap();
    assert!(result_at.is_some(), "similarity ~0.8 > threshold 0.79 → must match");

    // Threshold at 0.81 (just above similarity) → should NOT match.
    let result_above = storage.find_similar(&query, 0.81).unwrap();
    assert!(result_above.is_none(), "similarity ~0.8 < threshold 0.81 → must not match");
}

#[test]
fn test_find_similar_overwritten_embedding() {
    // Verify store_embedding replaces previous embedding (DELETE+INSERT).
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-overwrite", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let mut vec_a = vec![0.0_f32; EMBEDDING_DIMENSION];
    if let Some(first) = vec_a.first_mut() {
        *first = 1.0;
    }
    storage.store_embedding("obs-overwrite", &vec_a).unwrap();

    let mut vec_b = vec![0.0_f32; EMBEDDING_DIMENSION];
    if let Some(second) = vec_b.get_mut(1) {
        *second = 1.0;
    }
    storage.store_embedding("obs-overwrite", &vec_b).unwrap();

    // Query for e_1 → should match (new embedding)
    let result_b = storage.find_similar(&vec_b, 0.9).unwrap();
    assert!(result_b.is_some(), "overwritten embedding must be searchable");

    // Query for e_0 → should NOT match (old embedding replaced)
    let result_a = storage.find_similar(&vec_a, 0.9).unwrap();
    assert!(result_a.is_none(), "old embedding must be replaced, not found");
}

#[test]
fn test_store_embedding_rejects_zero_vector() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-zero-vec", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let zero_vec = vec![0.0_f32; EMBEDDING_DIMENSION];
    storage.store_embedding("obs-zero-vec", &zero_vec).unwrap();

    // Zero vector was rejected silently (Ok(())), so observation should have no embedding.
    // Verify by checking it appears in "without embeddings" list.
    let without = storage.get_observations_without_embeddings(100).unwrap();
    assert!(
        without.iter().any(|o| o.id == "obs-zero-vec"),
        "zero vector must be rejected — observation should still lack embedding"
    );
}

#[test]
fn test_find_similar_rejects_zero_vector() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-zero-query", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let norm: f32 = (EMBEDDING_DIMENSION as f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; EMBEDDING_DIMENSION];
    storage.store_embedding("obs-zero-query", &unit_vec).unwrap();

    let zero_query = vec![0.0_f32; EMBEDDING_DIMENSION];
    let result = storage.find_similar(&zero_query, 0.0).unwrap();
    assert!(result.is_none(), "zero vector query must return None");
}
