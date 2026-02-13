//! Tests for semantic deduplication: `union_dedup`, `merge_into_existing`,
//! `find_similar`, and `observation_embedding_text`.

use super::{create_test_observation, create_test_storage};
use opencode_mem_core::{observation_embedding_text, ObservationType, SimilarMatch};

use crate::storage::union_dedup;

// ---------------------------------------------------------------------------
// union_dedup
// ---------------------------------------------------------------------------

#[test]
fn test_union_dedup_merges_unique() {
    let existing = vec!["A".to_owned(), "B".to_owned(), "C".to_owned()];
    let newer = vec!["B".to_owned(), "C".to_owned(), "D".to_owned()];

    let result = union_dedup(&existing, &newer);

    // Unique items only, existing-first order preserved.
    assert_eq!(result, vec!["A", "B", "C", "D"]);
}

#[test]
fn test_union_dedup_empty_inputs() {
    let empty: Vec<String> = Vec::new();
    let some = vec!["x".to_owned()];

    // both empty
    assert!(union_dedup(&empty, &empty).is_empty());

    // existing empty → returns newer
    assert_eq!(union_dedup(&empty, &some), vec!["x"]);

    // newer empty → returns existing
    assert_eq!(union_dedup(&some, &empty), vec!["x"]);
}

// ---------------------------------------------------------------------------
// merge_into_existing
// ---------------------------------------------------------------------------

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_merge_into_existing_unions_facts_and_keywords() {
    let (storage, _dir) = create_test_storage();

    // obs1: short narrative, facts=["A","B"], keywords=["x"], files_read=["f1.rs"]
    let obs1 = opencode_mem_core::Observation::builder(
        "obs-merge-1".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Merge test".to_owned(),
    )
    .narrative("short")
    .facts(vec!["A".to_owned(), "B".to_owned()])
    .keywords(vec!["x".to_owned()])
    .files_read(vec!["f1.rs".to_owned()])
    .build();

    assert!(storage.save_observation(&obs1).unwrap());

    // obs2: longer narrative, overlapping + new facts/keywords/files
    let obs2 = opencode_mem_core::Observation::builder(
        "obs-merge-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Merge test newer".to_owned(),
    )
    .narrative("much longer narrative that should win")
    .facts(vec!["B".to_owned(), "C".to_owned()])
    .keywords(vec!["x".to_owned(), "y".to_owned()])
    .files_read(vec!["f2.rs".to_owned()])
    .build();

    storage.merge_into_existing("obs-merge-1", &obs2).unwrap();

    let merged = storage.get_by_id("obs-merge-1").unwrap().unwrap();
    assert_eq!(merged.facts, vec!["A", "B", "C"]);
    assert_eq!(merged.keywords, vec!["x", "y"]);
    assert_eq!(merged.files_read, vec!["f1.rs", "f2.rs"]);
    assert_eq!(merged.narrative.as_deref(), Some("much longer narrative that should win"));
}

#[test]
fn test_merge_into_existing_nonexistent_returns_error() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-dummy", "proj");
    let result = storage.merge_into_existing("nonexistent-id", &obs);

    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// find_similar
// ---------------------------------------------------------------------------

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_find_similar_returns_none_when_no_embeddings() {
    let (storage, _dir) = create_test_storage();

    // Empty DB, no embeddings stored → should return None.
    let vec_384: Vec<f32> = vec![1.0; 384];
    let result = storage.find_similar(&vec_384, 0.5).unwrap();
    assert!(result.is_none());
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_find_similar_returns_match_above_threshold() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-sim-1", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    // Normalized unit vector: [1/sqrt(384); 384]
    let norm: f32 = (384.0_f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; 384];
    storage.store_embedding("obs-sim-1", &unit_vec).unwrap();

    // Query with the identical vector → cosine similarity = 1.0.
    let result = storage.find_similar(&unit_vec, 0.9).unwrap();
    assert!(result.is_some(), "expected a SimilarMatch for identical vector");

    let m: SimilarMatch = result.unwrap();
    assert_eq!(m.observation_id, "obs-sim-1");
    assert!(m.similarity >= 0.9, "similarity {} should be >= 0.9", m.similarity);
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
#[expect(
    clippy::indexing_slicing,
    reason = "test code — vec length is 384, indices 0 and 1 are safe"
)]
fn test_find_similar_returns_none_below_threshold() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-orth-1", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    // Store embedding: e_1 direction (first component = 1.0, rest = 0.0)
    let mut vec_a = vec![0.0_f32; 384];
    vec_a[0] = 1.0;
    storage.store_embedding("obs-orth-1", &vec_a).unwrap();

    // Query with orthogonal vector: e_2 direction
    let mut vec_b = vec![0.0_f32; 384];
    vec_b[1] = 1.0;

    // Cosine similarity of orthogonal vectors = 0.0, well below 0.9.
    let result = storage.find_similar(&vec_b, 0.9).unwrap();
    assert!(result.is_none(), "orthogonal vectors should not match at threshold 0.9");
}

// ---------------------------------------------------------------------------
// observation_embedding_text
// ---------------------------------------------------------------------------

#[test]
fn test_observation_embedding_text() {
    let obs = opencode_mem_core::Observation::builder(
        "obs-emb-txt".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "MyTitle".to_owned(),
    )
    .narrative("narrative part")
    .facts(vec!["fact-a".to_owned(), "fact-b".to_owned()])
    .build();

    let text = observation_embedding_text(&obs);

    // Format: "{title} {narrative} {facts joined by space}"
    assert_eq!(text, "MyTitle narrative part fact-a fact-b");
}

#[test]
fn test_observation_embedding_text_no_narrative_no_facts() {
    let obs = opencode_mem_core::Observation::builder(
        "obs-emb-txt-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "TitleOnly".to_owned(),
    )
    .build();

    let text = observation_embedding_text(&obs);

    // No narrative → empty string, no facts → empty join.
    assert_eq!(text, "TitleOnly  ");
}
