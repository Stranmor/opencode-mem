//! Adversarial edge-case tests for embeddings subsystem.
//!
//! Covers: `clear_embeddings`, `store_embedding`, `find_similar`, `find_similar_many`,
//! `blob_to_f32_vec`, migration v15, and the `--reset-vec` CLI path.

#![expect(clippy::unwrap_used, reason = "test code")]

use super::{create_test_observation, create_test_storage};
use opencode_mem_core::EMBEDDING_DIMENSION;

// ─── VULNERABILITY #1 ──────────────────────────────────────────────
// clear_embeddings is not atomic: DROP TABLE + CREATE TABLE in execute_batch
// without a transaction. If process crashes between DROP and CREATE, the vec0
// table is gone permanently.

#[test]
fn test_clear_embeddings_is_idempotent() {
    // Property: clear_embeddings() called twice in sequence must not fail.
    // Proves: DROP IF EXISTS + CREATE IF NOT EXISTS both execute correctly.
    let (storage, _dir) = create_test_storage();

    // First call: table exists from migration, drop+create
    storage.clear_embeddings().unwrap();
    // Second call: table exists from first clear, drop+create again
    storage.clear_embeddings().unwrap();

    // Must be able to store embeddings after double clear
    let obs = create_test_observation("obs-idem-1", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let norm: f32 = (EMBEDDING_DIMENSION as f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; EMBEDDING_DIMENSION];
    storage.store_embedding("obs-idem-1", &unit_vec).unwrap();

    // Verify embedding is searchable
    let result = storage.find_similar(&unit_vec, 0.9).unwrap();
    assert!(result.is_some(), "embedding must be searchable after double clear");
}

// ─── VULNERABILITY #2 ──────────────────────────────────────────────
// clear_embeddings drops all embeddings. Subsequent find_similar must
// return None, not error or stale results.

#[test]
fn test_clear_embeddings_wipes_all_embeddings() {
    let (storage, _dir) = create_test_storage();

    // Store 3 observations with embeddings
    for i in 0..3_usize {
        let id = format!("obs-wipe-{i}");
        let obs = create_test_observation(&id, "proj");
        assert!(storage.save_observation(&obs).unwrap());

        let mut vec = vec![0.0_f32; EMBEDDING_DIMENSION];
        if let Some(elem) = vec.get_mut(i) {
            *elem = 1.0;
        }
        storage.store_embedding(&id, &vec).unwrap();
    }

    // Verify embeddings exist
    let without_before = storage.get_observations_without_embeddings(100).unwrap();
    assert_eq!(without_before.len(), 0, "all 3 should have embeddings");

    // Clear all
    storage.clear_embeddings().unwrap();

    // After clear, all observations must appear as "without embeddings"
    let without_after = storage.get_observations_without_embeddings(100).unwrap();
    assert_eq!(without_after.len(), 3, "all 3 should need re-embedding after clear");

    // find_similar must return None (no embeddings exist)
    let mut query = vec![0.0_f32; EMBEDDING_DIMENSION];
    if let Some(first) = query.first_mut() {
        *first = 1.0;
    }
    let result = storage.find_similar(&query, 0.0).unwrap();
    assert!(result.is_none(), "find_similar must return None after clear_embeddings");
}

// ─── VULNERABILITY #3 ──────────────────────────────────────────────
// store_embedding for a non-existent observation_id: the rowid lookup
// (SELECT rowid FROM observations WHERE id = ?1) will fail with
// QueryReturnedNoRows, causing an opaque error. This is the expected
// behavior — but it must be an Error, not a panic.

#[test]
fn test_store_embedding_nonexistent_observation_returns_error() {
    let (storage, _dir) = create_test_storage();

    let vec = vec![1.0_f32; EMBEDDING_DIMENSION];
    let result = storage.store_embedding("does-not-exist", &vec);
    assert!(
        result.is_err(),
        "store_embedding for non-existent observation must return Err, not Ok or panic"
    );
}

// ─── VULNERABILITY #4 ──────────────────────────────────────────────
// store_embedding with wrong dimension must be rejected.
// Property: dimension mismatch → Error (not silent corruption).

#[test]
fn test_store_embedding_wrong_dimension_rejected() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-wrong-dim", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    // Too few dimensions
    let short_vec = vec![1.0_f32; EMBEDDING_DIMENSION - 1];
    let result = storage.store_embedding("obs-wrong-dim", &short_vec);
    assert!(result.is_err(), "embedding with EMBEDDING_DIMENSION-1 must be rejected");

    // Too many dimensions
    let long_vec = vec![1.0_f32; EMBEDDING_DIMENSION + 1];
    let result = storage.store_embedding("obs-wrong-dim", &long_vec);
    assert!(result.is_err(), "embedding with EMBEDDING_DIMENSION+1 must be rejected");

    // Empty
    let empty_vec: Vec<f32> = vec![];
    let result = storage.store_embedding("obs-wrong-dim", &empty_vec);
    assert!(result.is_err(), "empty embedding must be rejected");
}

// ─── VULNERABILITY #5 ──────────────────────────────────────────────
// find_similar_many with limit=0 should return empty vec, not error.

#[test]
fn test_find_similar_many_limit_zero() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-limit0", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let norm: f32 = (EMBEDDING_DIMENSION as f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; EMBEDDING_DIMENSION];
    storage.store_embedding("obs-limit0", &unit_vec).unwrap();

    // limit=0: should return empty vec, not error or all results
    let result = storage.find_similar_many(&unit_vec, 0.0, 0).unwrap();
    assert!(result.is_empty(), "limit=0 must return empty vec");
}

// ─── VULNERABILITY #6 ──────────────────────────────────────────────
// find_similar_many must return results ordered by similarity descending.
// Property: monotonicity — each result.similarity >= next result.similarity.

#[test]
#[expect(
    clippy::indexing_slicing,
    reason = "test code — indices 0..4 are safe for EMBEDDING_DIMENSION-sized vec"
)]
fn test_find_similar_many_ordering_monotonic() {
    let (storage, _dir) = create_test_storage();

    // Create 5 observations with different directions
    for i in 0..5_usize {
        let id = format!("obs-ord-{i}");
        let obs = create_test_observation(&id, "proj");
        assert!(storage.save_observation(&obs).unwrap());

        let mut vec = vec![0.0_f32; EMBEDDING_DIMENSION];
        if let Some(v) = vec.get_mut(i) {
            *v = 1.0;
        }
        storage.store_embedding(&id, &vec).unwrap();
    }

    // Query in e_0 direction
    let mut query = vec![0.0_f32; EMBEDDING_DIMENSION];
    query[0] = 1.0;

    let results = storage.find_similar_many(&query, 0.0, 5).unwrap();

    // Property: similarity scores must be monotonically non-increasing
    for window in results.windows(2) {
        let (a, b) = (&window[0], &window[1]);
        assert!(
            a.similarity >= b.similarity,
            "Results must be ordered by similarity descending: {} >= {}",
            a.similarity,
            b.similarity
        );
    }
}

// ─── VULNERABILITY #7 ──────────────────────────────────────────────
// clear_embeddings then store_embedding then find_similar:
// Roundtrip property after reset.

#[test]
fn test_clear_then_store_then_find_roundtrip() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-roundtrip", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    // Store embedding
    let norm: f32 = (EMBEDDING_DIMENSION as f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; EMBEDDING_DIMENSION];
    storage.store_embedding("obs-roundtrip", &unit_vec).unwrap();

    // Clear all
    storage.clear_embeddings().unwrap();

    // Re-store the same embedding
    storage.store_embedding("obs-roundtrip", &unit_vec).unwrap();

    // Must be findable again
    let result = storage.find_similar(&unit_vec, 0.9).unwrap();
    assert!(result.is_some(), "roundtrip clear→store→find must work");
    assert_eq!(result.unwrap().observation_id, "obs-roundtrip");
}

// ─── VULNERABILITY #8 ──────────────────────────────────────────────
// get_observations_without_embeddings after clear_embeddings must return
// ALL observations (observations exist, vec0 table is empty).

#[test]
fn test_get_observations_without_embeddings_after_clear() {
    let (storage, _dir) = create_test_storage();

    // Create 5 observations, embed 3 of them
    for i in 0..5_u32 {
        let id = format!("obs-notemb-{i}");
        let obs = create_test_observation(&id, "proj");
        assert!(storage.save_observation(&obs).unwrap());
    }

    for i in 0..3_usize {
        let id = format!("obs-notemb-{i}");
        let mut vec = vec![0.0_f32; EMBEDDING_DIMENSION];
        if let Some(elem) = vec.get_mut(i) {
            *elem = 1.0;
        }
        storage.store_embedding(&id, &vec).unwrap();
    }

    // Before clear: 2 without embeddings
    let before = storage.get_observations_without_embeddings(100).unwrap();
    assert_eq!(before.len(), 2);

    // After clear: all 5 without embeddings
    storage.clear_embeddings().unwrap();
    let after = storage.get_observations_without_embeddings(100).unwrap();
    assert_eq!(after.len(), 5);
}

// ─── VULNERABILITY #9 ──────────────────────────────────────────────
// get_embeddings_for_ids after clear_embeddings must return empty vec.

#[test]
fn test_get_embeddings_for_ids_after_clear() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-getids-1", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let norm: f32 = (EMBEDDING_DIMENSION as f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; EMBEDDING_DIMENSION];
    storage.store_embedding("obs-getids-1", &unit_vec).unwrap();

    // Before clear: should have embedding
    let before = storage.get_embeddings_for_ids(&["obs-getids-1".to_owned()]).unwrap();
    assert_eq!(before.len(), 1);

    // After clear
    storage.clear_embeddings().unwrap();
    let after = storage.get_embeddings_for_ids(&["obs-getids-1".to_owned()]).unwrap();
    assert!(after.is_empty(), "get_embeddings_for_ids must return empty after clear");
}

// ─── VULNERABILITY #10 ─────────────────────────────────────────────
// Concurrent access: store_embedding immediately after clear_embeddings.
// Simulates the race condition where service is running while CLI
// runs backfill --reset-vec.

#[test]
fn test_store_embedding_during_concurrent_clear() {
    let (storage, _dir) = create_test_storage();

    // Create observation first
    let obs = create_test_observation("obs-race-1", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let norm: f32 = (EMBEDDING_DIMENSION as f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; EMBEDDING_DIMENSION];

    // Store initial embedding
    storage.store_embedding("obs-race-1", &unit_vec).unwrap();

    // Simulate race: clear happens, then another store attempts
    storage.clear_embeddings().unwrap();

    // This must NOT panic — rowid from observations table still exists,
    // vec0 table is recreated (empty), INSERT into vec0 must work.
    let result = storage.store_embedding("obs-race-1", &unit_vec);
    assert!(result.is_ok(), "store_embedding after clear must succeed, got: {:?}", result.err());

    // And the embedding must be findable
    let found = storage.find_similar(&unit_vec, 0.9).unwrap();
    assert!(found.is_some(), "embedding stored after clear must be findable");
}

// ─── VULNERABILITY #11 ─────────────────────────────────────────────
// find_similar_many with threshold > 1.0: no results should match
// since cosine similarity is bounded to [-1, 1].

#[test]
fn test_find_similar_many_threshold_above_one() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-thresh-gt1", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let norm: f32 = (EMBEDDING_DIMENSION as f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; EMBEDDING_DIMENSION];
    storage.store_embedding("obs-thresh-gt1", &unit_vec).unwrap();

    // threshold > 1.0: physically impossible to match
    let result = storage.find_similar_many(&unit_vec, 1.01, 10).unwrap();
    assert!(result.is_empty(), "threshold > 1.0 must yield no results");

    let single = storage.find_similar(&unit_vec, 1.01).unwrap();
    assert!(single.is_none(), "find_similar with threshold > 1.0 must return None");
}

// ─── VULNERABILITY #12 ─────────────────────────────────────────────
// find_similar with negative threshold: should match everything
// (cosine similarity >= -1.0 always).

#[test]
fn test_find_similar_negative_threshold() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-neg-thresh", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let mut vec = vec![0.0_f32; EMBEDDING_DIMENSION];
    if let Some(first) = vec.first_mut() {
        *first = 1.0;
    }
    storage.store_embedding("obs-neg-thresh", &vec).unwrap();

    // Negative threshold: any cosine similarity >= -1.0 (always true)
    let result = storage.find_similar(&vec, -1.0).unwrap();
    assert!(result.is_some(), "negative threshold should match anything");
}

// ─── VULNERABILITY #13 ─────────────────────────────────────────────
// Embedding with NaN values: should not corrupt the database or
// produce bogus search results.

#[test]
fn test_store_embedding_nan_values() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-nan", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let mut nan_vec = vec![0.0_f32; EMBEDDING_DIMENSION];
    if let Some(first) = nan_vec.first_mut() {
        *first = f32::NAN;
    }

    // NaN in embedding must be rejected at the guard level.
    let result = storage.store_embedding("obs-nan", &nan_vec);
    assert!(result.is_err(), "NaN embedding should be rejected");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("NaN or Infinity"),
        "error should mention NaN or Infinity, got: {err_msg}"
    );
}

// ─── VULNERABILITY #14 ─────────────────────────────────────────────
// Embedding with infinity values: extreme float values must not corrupt.

#[test]
fn test_store_embedding_infinity_values() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-inf", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let mut inf_vec = vec![0.0_f32; EMBEDDING_DIMENSION];
    if let Some(first) = inf_vec.first_mut() {
        *first = f32::INFINITY;
    }

    // Infinity in embedding must be rejected at the guard level.
    let result = storage.store_embedding("obs-inf", &inf_vec);
    assert!(result.is_err(), "Infinity embedding should be rejected");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("NaN or Infinity"),
        "error should mention NaN or Infinity, got: {err_msg}"
    );
}

// ─── VULNERABILITY #15 ─────────────────────────────────────────────
// store_embedding idempotency: storing same embedding twice must not
// create duplicate entries in vec0.

#[test]
fn test_store_embedding_idempotent() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-idemp-emb", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let norm: f32 = (EMBEDDING_DIMENSION as f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; EMBEDDING_DIMENSION];

    // Store twice
    storage.store_embedding("obs-idemp-emb", &unit_vec).unwrap();
    storage.store_embedding("obs-idemp-emb", &unit_vec).unwrap();

    // find_similar_many should return exactly 1 result, not 2
    let results = storage.find_similar_many(&unit_vec, 0.9, 10).unwrap();
    assert_eq!(
        results.len(),
        1,
        "double store must not create duplicate vec0 entries, got {}",
        results.len()
    );
}

// ─── VULNERABILITY #16 ─────────────────────────────────────────────
// clear_embeddings preserves observation data: only vec0 table is dropped,
// observations table must remain intact.

#[test]
fn test_clear_embeddings_preserves_observations() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-preserve", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let norm: f32 = (EMBEDDING_DIMENSION as f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; EMBEDDING_DIMENSION];
    storage.store_embedding("obs-preserve", &unit_vec).unwrap();

    // Clear embeddings
    storage.clear_embeddings().unwrap();

    // Observation must still exist
    let retrieved = storage.get_by_id("obs-preserve").unwrap();
    assert!(retrieved.is_some(), "clear_embeddings must NOT delete observations");
    assert_eq!(retrieved.unwrap().title, "Test observation obs-preserve");

    // Stats must still show 1 observation
    let stats = storage.get_stats().unwrap();
    assert_eq!(stats.observation_count, 1);
}
