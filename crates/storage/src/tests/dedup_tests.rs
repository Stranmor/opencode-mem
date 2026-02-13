//! Stress tests for semantic deduplication: `union_dedup`, `merge_into_existing`,
//! `find_similar`, and `observation_embedding_text`.
//!
//! Coverage targets:
//! - Edge cases: empty inputs, very long strings, special characters, unicode
//! - Boundary conditions: threshold at exact boundary, 0.0, 1.0
//! - False positive detection: tests that could pass with broken implementation
//! - Narrative merge: all 4 match arms (both Some longer/shorter/equal, None combinations)
//! - Multiple embeddings: best-match selection, not just single-embedding scenarios

use super::{create_test_observation, create_test_storage};
use opencode_mem_core::{
    observation_embedding_text, union_dedup, Concept, ObservationType, SimilarMatch,
};

// ===========================================================================
// union_dedup — pure function tests
// ===========================================================================

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

#[test]
fn test_union_dedup_preserves_existing_first_order() {
    // Existing order must be preserved, newer items appended in their order.
    let existing = vec!["C".to_owned(), "A".to_owned(), "B".to_owned()];
    let newer = vec!["D".to_owned(), "A".to_owned(), "E".to_owned()];

    let result = union_dedup(&existing, &newer);
    assert_eq!(result, vec!["C", "A", "B", "D", "E"]);
}

#[test]
fn test_union_dedup_all_duplicates() {
    // When newer is entirely contained in existing, result = existing.
    let existing = vec!["A".to_owned(), "B".to_owned(), "C".to_owned()];
    let newer = vec!["C".to_owned(), "B".to_owned(), "A".to_owned()];

    let result = union_dedup(&existing, &newer);
    assert_eq!(result, vec!["A", "B", "C"]);
}

#[test]
fn test_union_dedup_duplicates_within_single_input() {
    // If existing itself has duplicates, only the first occurrence survives.
    let existing = vec!["A".to_owned(), "B".to_owned(), "A".to_owned()];
    let newer = vec!["B".to_owned(), "C".to_owned()];

    let result = union_dedup(&existing, &newer);
    assert_eq!(result, vec!["A", "B", "C"]);
}

#[test]
fn test_union_dedup_case_sensitive() {
    // union_dedup is case-sensitive: "a" and "A" are different items.
    let existing = vec!["a".to_owned()];
    let newer = vec!["A".to_owned()];

    let result = union_dedup(&existing, &newer);
    assert_eq!(result, vec!["a", "A"], "dedup must be case-sensitive");
}

#[test]
fn test_union_dedup_unicode_and_special_chars() {
    let existing = vec!["日本語".to_owned(), "émoji: 🦀".to_owned()];
    let newer = vec!["émoji: 🦀".to_owned(), "中文".to_owned(), "path/with spaces".to_owned()];

    let result = union_dedup(&existing, &newer);
    assert_eq!(result, vec!["日本語", "émoji: 🦀", "中文", "path/with spaces"]);
}

#[test]
fn test_union_dedup_whitespace_variants_not_collapsed() {
    // " x" and "x" and "x " are different strings — no trimming.
    let existing = vec![" x".to_owned(), "x".to_owned()];
    let newer = vec!["x ".to_owned()];

    let result = union_dedup(&existing, &newer);
    assert_eq!(result, vec![" x", "x", "x "]);
}

#[test]
fn test_union_dedup_empty_strings() {
    // Empty string is a valid distinct item.
    let existing = vec!["".to_owned(), "a".to_owned()];
    let newer = vec!["".to_owned(), "b".to_owned()];

    let result = union_dedup(&existing, &newer);
    assert_eq!(result, vec!["", "a", "b"]);
}

#[expect(clippy::indexing_slicing, reason = "test code — length verified by assert_eq above")]
#[test]
fn test_union_dedup_very_long_strings() {
    let long_a = "x".repeat(10_000);
    let long_b = "y".repeat(10_000);
    let existing = vec![long_a.clone()];
    let newer = vec![long_a.clone(), long_b.clone()];

    let result = union_dedup(&existing, &newer);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], long_a);
    assert_eq!(result[1], long_b);
}

#[expect(clippy::indexing_slicing, reason = "test code — length verified by assert_eq above")]
#[test]
fn test_union_dedup_large_input_count() {
    // 1000 items — verify no performance pathology and correctness.
    let existing: Vec<String> = (0..500).map(|i| format!("item-{i}")).collect();
    let newer: Vec<String> = (250..750).map(|i| format!("item-{i}")).collect();

    let result = union_dedup(&existing, &newer);

    // 0..749 = 750 unique items
    assert_eq!(result.len(), 750);
    // First item from existing preserved at position 0
    assert_eq!(result[0], "item-0");
    // Last item from newer appended at end
    assert_eq!(result[749], "item-749");
}

// ===========================================================================
// merge_into_existing — DB integration tests
// ===========================================================================

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

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_merge_narrative_existing_longer_wins() {
    let (storage, _dir) = create_test_storage();

    let obs1 = opencode_mem_core::Observation::builder(
        "obs-narr-long".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Narrative length test".to_owned(),
    )
    .narrative("this existing narrative is much longer than the newer one")
    .build();

    assert!(storage.save_observation(&obs1).unwrap());

    let obs2 = opencode_mem_core::Observation::builder(
        "obs-narr-short".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Shorter".to_owned(),
    )
    .narrative("short")
    .build();

    storage.merge_into_existing("obs-narr-long", &obs2).unwrap();

    let merged = storage.get_by_id("obs-narr-long").unwrap().unwrap();
    assert_eq!(
        merged.narrative.as_deref(),
        Some("this existing narrative is much longer than the newer one"),
        "existing narrative is longer → must be preserved"
    );
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_merge_narrative_equal_length_existing_wins() {
    let (storage, _dir) = create_test_storage();

    let obs1 = opencode_mem_core::Observation::builder(
        "obs-eq-narr".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Equal narrative test".to_owned(),
    )
    .narrative("AAAA") // len = 4
    .build();

    assert!(storage.save_observation(&obs1).unwrap());

    let obs2 = opencode_mem_core::Observation::builder(
        "obs-eq-narr-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Other".to_owned(),
    )
    .narrative("BBBB") // len = 4 — same length
    .build();

    storage.merge_into_existing("obs-eq-narr", &obs2).unwrap();

    let merged = storage.get_by_id("obs-eq-narr").unwrap().unwrap();
    assert_eq!(
        merged.narrative.as_deref(),
        Some("AAAA"),
        "equal length → existing narrative wins (implementation uses strict >)"
    );
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_merge_narrative_both_none() {
    let (storage, _dir) = create_test_storage();

    let obs1 = opencode_mem_core::Observation::builder(
        "obs-no-narr".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "No narrative test".to_owned(),
    )
    .facts(vec!["A".to_owned()])
    .build();

    assert!(storage.save_observation(&obs1).unwrap());

    let obs2 = opencode_mem_core::Observation::builder(
        "obs-no-narr-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Other".to_owned(),
    )
    .facts(vec!["B".to_owned()])
    .build();

    storage.merge_into_existing("obs-no-narr", &obs2).unwrap();

    let merged = storage.get_by_id("obs-no-narr").unwrap().unwrap();
    assert!(merged.narrative.is_none(), "both narratives None → result must be None");
    assert_eq!(merged.facts, vec!["A", "B"]);
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_merge_narrative_existing_none_newer_some() {
    let (storage, _dir) = create_test_storage();

    let obs1 = opencode_mem_core::Observation::builder(
        "obs-none-some".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Existing has no narrative".to_owned(),
    )
    .build();

    assert!(storage.save_observation(&obs1).unwrap());

    let obs2 = opencode_mem_core::Observation::builder(
        "obs-none-some-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Other".to_owned(),
    )
    .narrative("newer provides narrative")
    .build();

    storage.merge_into_existing("obs-none-some", &obs2).unwrap();

    let merged = storage.get_by_id("obs-none-some").unwrap().unwrap();
    assert_eq!(
        merged.narrative.as_deref(),
        Some("newer provides narrative"),
        "existing=None, newer=Some → newer wins"
    );
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_merge_narrative_existing_some_newer_none() {
    let (storage, _dir) = create_test_storage();

    let obs1 = opencode_mem_core::Observation::builder(
        "obs-some-none".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Existing has narrative".to_owned(),
    )
    .narrative("existing narrative preserved")
    .build();

    assert!(storage.save_observation(&obs1).unwrap());

    let obs2 = opencode_mem_core::Observation::builder(
        "obs-some-none-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Other".to_owned(),
    )
    .build(); // no narrative

    storage.merge_into_existing("obs-some-none", &obs2).unwrap();

    let merged = storage.get_by_id("obs-some-none").unwrap().unwrap();
    assert_eq!(
        merged.narrative.as_deref(),
        Some("existing narrative preserved"),
        "existing=Some, newer=None → existing wins"
    );
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_merge_files_modified_union() {
    // The original test only verified files_read. Verify files_modified too.
    let (storage, _dir) = create_test_storage();

    let obs1 = opencode_mem_core::Observation::builder(
        "obs-fmod-1".to_owned(),
        "session-1".to_owned(),
        ObservationType::Change,
        "Files modified test".to_owned(),
    )
    .files_modified(vec!["src/main.rs".to_owned(), "Cargo.toml".to_owned()])
    .build();

    assert!(storage.save_observation(&obs1).unwrap());

    let obs2 = opencode_mem_core::Observation::builder(
        "obs-fmod-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Change,
        "Other".to_owned(),
    )
    .files_modified(vec!["Cargo.toml".to_owned(), "src/lib.rs".to_owned()])
    .build();

    storage.merge_into_existing("obs-fmod-1", &obs2).unwrap();

    let merged = storage.get_by_id("obs-fmod-1").unwrap().unwrap();
    assert_eq!(merged.files_modified, vec!["src/main.rs", "Cargo.toml", "src/lib.rs"]);
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_merge_uses_later_created_at() {
    use chrono::{TimeZone, Utc};

    let (storage, _dir) = create_test_storage();

    let earlier = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    let later = Utc.with_ymd_and_hms(2026, 6, 15, 12, 0, 0).unwrap();

    let obs1 = opencode_mem_core::Observation::builder(
        "obs-ts-1".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Timestamp test".to_owned(),
    )
    .created_at(earlier)
    .build();

    assert!(storage.save_observation(&obs1).unwrap());

    let obs2 = opencode_mem_core::Observation::builder(
        "obs-ts-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Other".to_owned(),
    )
    .created_at(later)
    .build();

    storage.merge_into_existing("obs-ts-1", &obs2).unwrap();

    let merged = storage.get_by_id("obs-ts-1").unwrap().unwrap();
    // max(earlier, later) = later
    assert_eq!(merged.created_at, later, "created_at must be max of both");
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
#[expect(clippy::indexing_slicing, reason = "test code — length verified by assert_eq above")]
fn test_merge_special_chars_in_facts() {
    let (storage, _dir) = create_test_storage();

    let obs1 = opencode_mem_core::Observation::builder(
        "obs-special-1".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Special chars merge".to_owned(),
    )
    .facts(vec![r#"contains "quotes""#.to_owned(), "has\nnewline".to_owned()])
    .build();

    assert!(storage.save_observation(&obs1).unwrap());

    let obs2 = opencode_mem_core::Observation::builder(
        "obs-special-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Other".to_owned(),
    )
    .facts(vec![
        "has\nnewline".to_owned(), // duplicate
        "emoji: 🦀".to_owned(),
    ])
    .build();

    storage.merge_into_existing("obs-special-1", &obs2).unwrap();

    let merged = storage.get_by_id("obs-special-1").unwrap().unwrap();
    assert_eq!(merged.facts.len(), 3);
    assert_eq!(merged.facts[0], r#"contains "quotes""#);
    assert_eq!(merged.facts[1], "has\nnewline");
    assert_eq!(merged.facts[2], "emoji: 🦀");
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_merge_idempotent_second_merge_no_change() {
    // Merging the same data twice should produce identical result.
    let (storage, _dir) = create_test_storage();

    let obs1 = opencode_mem_core::Observation::builder(
        "obs-idem-1".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Idempotent merge".to_owned(),
    )
    .narrative("existing")
    .facts(vec!["A".to_owned()])
    .keywords(vec!["k1".to_owned()])
    .build();

    assert!(storage.save_observation(&obs1).unwrap());

    let obs2 = opencode_mem_core::Observation::builder(
        "obs-idem-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Other".to_owned(),
    )
    .narrative("longer newer narrative here")
    .facts(vec!["A".to_owned(), "B".to_owned()])
    .keywords(vec!["k1".to_owned(), "k2".to_owned()])
    .build();

    storage.merge_into_existing("obs-idem-1", &obs2).unwrap();
    let after_first = storage.get_by_id("obs-idem-1").unwrap().unwrap();

    // Second merge with same data
    storage.merge_into_existing("obs-idem-1", &obs2).unwrap();
    let after_second = storage.get_by_id("obs-idem-1").unwrap().unwrap();

    assert_eq!(after_first.facts, after_second.facts);
    assert_eq!(after_first.keywords, after_second.keywords);
    assert_eq!(after_first.narrative, after_second.narrative);
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_merge_preserves_non_merged_fields() {
    // Regression guard: merge only touches facts, keywords, files_read,
    // files_modified, narrative, created_at. All other fields must be preserved.
    let (storage, _dir) = create_test_storage();

    let obs1 = opencode_mem_core::Observation::builder(
        "obs-immut".to_owned(),
        "session-immut".to_owned(),
        ObservationType::Discovery,
        "Original Title Untouched".to_owned(),
    )
    .project("original-project")
    .subtitle("original subtitle")
    .narrative("short")
    .prompt_number(42)
    .discovery_tokens(999)
    .build();

    assert!(storage.save_observation(&obs1).unwrap());

    let obs2 = opencode_mem_core::Observation::builder(
        "obs-immut-2".to_owned(),
        "session-other".to_owned(),
        ObservationType::Bugfix,
        "Different Title".to_owned(),
    )
    .project("different-project")
    .subtitle("different subtitle")
    .narrative("longer narrative wins")
    .prompt_number(99)
    .discovery_tokens(1)
    .build();

    storage.merge_into_existing("obs-immut", &obs2).unwrap();

    let merged = storage.get_by_id("obs-immut").unwrap().unwrap();

    // These fields must NOT change during merge:
    assert_eq!(merged.id, "obs-immut", "id must be preserved");
    assert_eq!(merged.session_id, "session-immut", "session_id must be preserved");
    assert_eq!(merged.title, "Original Title Untouched", "title must be preserved");
    assert_eq!(merged.observation_type, ObservationType::Discovery, "type must be preserved");
    assert_eq!(merged.project.as_deref(), Some("original-project"), "project must be preserved");
    assert_eq!(merged.subtitle.as_deref(), Some("original subtitle"), "subtitle must be preserved");
    assert_eq!(merged.prompt_number, Some(42), "prompt_number must be preserved");
    assert_eq!(merged.discovery_tokens, Some(999), "discovery_tokens must be preserved");

    // Narrative was merged (newer is longer → wins):
    assert_eq!(merged.narrative.as_deref(), Some("longer narrative wins"));
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_merge_narrative_unicode_byte_vs_char_length() {
    // Documents that narrative comparison uses byte length, not char count.
    // "日本語" = 3 chars, 9 bytes. "hello" = 5 chars, 5 bytes.
    // Byte-length comparison: Japanese string wins (9 > 5) despite fewer chars.
    let (storage, _dir) = create_test_storage();

    let obs1 = opencode_mem_core::Observation::builder(
        "obs-unicode-narr".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Unicode narrative test".to_owned(),
    )
    .narrative("hello") // 5 bytes, 5 chars
    .build();

    assert!(storage.save_observation(&obs1).unwrap());

    let obs2 = opencode_mem_core::Observation::builder(
        "obs-unicode-narr-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Other".to_owned(),
    )
    .narrative("日本語") // 9 bytes, 3 chars
    .build();

    storage.merge_into_existing("obs-unicode-narr", &obs2).unwrap();

    let merged = storage.get_by_id("obs-unicode-narr").unwrap().unwrap();
    // Byte-length comparison: 9 > 5, so Japanese wins.
    // This documents current behavior — narrative length uses .len() (bytes).
    assert_eq!(
        merged.narrative.as_deref(),
        Some("日本語"),
        "byte-length comparison: Japanese (9 bytes) > ASCII (5 bytes)"
    );
}
// ===========================================================================
// find_similar — vector similarity tests
// ===========================================================================

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

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_find_similar_empty_embedding_returns_none() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-empty-emb", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let norm: f32 = (384.0_f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; 384];
    storage.store_embedding("obs-empty-emb", &unit_vec).unwrap();

    // Empty query embedding → early return None (implementation guard).
    let empty: Vec<f32> = vec![];
    let result = storage.find_similar(&empty, 0.0).unwrap();
    assert!(result.is_none(), "empty embedding must return None");
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_find_similar_threshold_zero_matches_anything() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-thresh0", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    // Store any normalized embedding
    let norm: f32 = (384.0_f32).sqrt();
    let stored: Vec<f32> = vec![1.0 / norm; 384];
    storage.store_embedding("obs-thresh0", &stored).unwrap();

    // Query with a DIFFERENT (but not orthogonal) vector, threshold = 0.0
    // Even low similarity should pass threshold=0.0
    let mut query = vec![0.5 / norm; 384];
    if let Some(first) = query.first_mut() {
        *first = 0.9; // skew direction slightly
    }

    let result = storage.find_similar(&query, 0.0).unwrap();
    assert!(result.is_some(), "threshold 0.0 should match any non-orthogonal vector");
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_find_similar_threshold_one_requires_exact_match() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-thresh1", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let norm: f32 = (384.0_f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; 384];
    storage.store_embedding("obs-thresh1", &unit_vec).unwrap();

    // Identical vector with threshold=1.0 should match (similarity=1.0, >= 1.0).
    let result_exact = storage.find_similar(&unit_vec, 1.0).unwrap();
    assert!(result_exact.is_some(), "identical vector at threshold=1.0 must match");

    // Slightly different vector with threshold=1.0 should NOT match.
    let mut slightly_off = unit_vec.clone();
    if let Some(first) = slightly_off.first_mut() {
        *first += 0.01; // perturb slightly
    }
    let result_off = storage.find_similar(&slightly_off, 1.0).unwrap();
    assert!(result_off.is_none(), "perturbed vector at threshold=1.0 must NOT match");
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
#[expect(clippy::indexing_slicing, reason = "test code — vec length is 384, indices 0..3 are safe")]
fn test_find_similar_multiple_embeddings_returns_best_match() {
    let (storage, _dir) = create_test_storage();

    // Store 3 observations with different embeddings.
    for i in 0..3_usize {
        let id = format!("obs-multi-{i}");
        let obs = create_test_observation(&id, "proj");
        assert!(storage.save_observation(&obs).unwrap());

        // Each vector points in e_i direction (unit vector along axis i).
        let mut vec = vec![0.0_f32; 384];
        vec[i] = 1.0;
        storage.store_embedding(&id, &vec).unwrap();
    }

    // Query for e_0 direction → should match obs-multi-0 with similarity=1.0
    let mut query = vec![0.0_f32; 384];
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
#[expect(clippy::unwrap_used, reason = "test code")]
#[expect(
    clippy::indexing_slicing,
    reason = "test code — vec length is 384, indices 0 and 1 are safe"
)]
fn test_find_similar_exact_threshold_boundary() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-boundary", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    // Craft a known similarity. For two unit vectors at angle θ:
    // cos(θ) = dot(a, b) / (|a| * |b|)
    // If a = [1, 0, 0...] and b = [cos(θ), sin(θ), 0...], similarity = cos(θ)
    //
    // Want similarity = 0.8 exactly: cos(θ) = 0.8, sin(θ) = 0.6
    let mut stored = vec![0.0_f32; 384];
    stored[0] = 1.0;
    storage.store_embedding("obs-boundary", &stored).unwrap();

    let mut query = vec![0.0_f32; 384];
    query[0] = 0.8;
    query[1] = 0.6; // |query| = sqrt(0.64 + 0.36) = 1.0, cos sim = 0.8

    // Threshold exactly at similarity → should match (>= comparison).
    let result_at = storage.find_similar(&query, 0.8).unwrap();
    assert!(result_at.is_some(), "similarity == threshold (0.8) → must match (>= comparison)");

    // Threshold just above → should NOT match.
    let result_above = storage.find_similar(&query, 0.81).unwrap();
    assert!(result_above.is_none(), "similarity 0.8 < threshold 0.81 → must not match");
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_find_similar_overwritten_embedding() {
    // Verify store_embedding replaces previous embedding (DELETE+INSERT).
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-overwrite", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    // Store initial embedding in e_0 direction
    let mut vec_a = vec![0.0_f32; 384];
    if let Some(first) = vec_a.first_mut() {
        *first = 1.0;
    }
    storage.store_embedding("obs-overwrite", &vec_a).unwrap();

    // Overwrite with e_1 direction
    let mut vec_b = vec![0.0_f32; 384];
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

// ===========================================================================
// observation_embedding_text — pure function tests
// ===========================================================================

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

    // Implementation: format!("{} {} {}", title, "", "")
    // Produces "TitleOnly  " (two trailing spaces).
    // This is a known artifact of the format! macro when narrative and facts are empty.
    // The test validates actual behavior, not ideal behavior.
    assert_eq!(text, "TitleOnly  ");
}

#[test]
fn test_observation_embedding_text_narrative_only_no_facts() {
    let obs = opencode_mem_core::Observation::builder(
        "obs-emb-narr-only".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Title".to_owned(),
    )
    .narrative("some narrative")
    .build();

    let text = observation_embedding_text(&obs);
    // facts is empty → join("") = "", so trailing space after narrative.
    assert_eq!(text, "Title some narrative ");
}

#[test]
fn test_observation_embedding_text_facts_only_no_narrative() {
    let obs = opencode_mem_core::Observation::builder(
        "obs-emb-facts-only".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Title".to_owned(),
    )
    .facts(vec!["fact1".to_owned()])
    .build();

    let text = observation_embedding_text(&obs);
    // narrative is None → unwrap_or("") → empty string, then space, then facts.
    assert_eq!(text, "Title  fact1");
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_merge_into_existing_includes_concepts() {
    let (storage, _dir) = create_test_storage();

    let obs1 = opencode_mem_core::Observation::builder(
        "obs-concept-1".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Concepts merge test".to_owned(),
    )
    .concepts(vec![Concept::HowItWorks, Concept::Gotcha])
    .build();

    assert!(storage.save_observation(&obs1).unwrap());

    let obs2 = opencode_mem_core::Observation::builder(
        "obs-concept-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Other".to_owned(),
    )
    .concepts(vec![Concept::Gotcha, Concept::Pattern, Concept::TradeOff])
    .build();

    storage.merge_into_existing("obs-concept-1", &obs2).unwrap();

    let merged = storage.get_by_id("obs-concept-1").unwrap().unwrap();
    assert_eq!(
        merged.concepts,
        vec![Concept::HowItWorks, Concept::Gotcha, Concept::Pattern, Concept::TradeOff],
        "concepts must be unioned with existing-first order, duplicates removed"
    );
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_store_embedding_rejects_zero_vector() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-zero-vec", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let zero_vec = vec![0.0_f32; 384];
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
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_find_similar_rejects_zero_vector() {
    let (storage, _dir) = create_test_storage();

    let obs = create_test_observation("obs-zero-query", "proj");
    assert!(storage.save_observation(&obs).unwrap());

    let norm: f32 = (384.0_f32).sqrt();
    let unit_vec: Vec<f32> = vec![1.0 / norm; 384];
    storage.store_embedding("obs-zero-query", &unit_vec).unwrap();

    // Query with zero vector → should return None (guard).
    let zero_query = vec![0.0_f32; 384];
    let result = storage.find_similar(&zero_query, 0.0).unwrap();
    assert!(result.is_none(), "zero vector query must return None");
}

#[test]
fn test_observation_embedding_text_unicode_content() {
    let obs = opencode_mem_core::Observation::builder(
        "obs-emb-unicode".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "🦀 Rust 日本語".to_owned(),
    )
    .narrative("narrative with émojis 👍")
    .facts(vec!["fact: Ü".to_owned()])
    .build();

    let text = observation_embedding_text(&obs);
    assert_eq!(text, "🦀 Rust 日本語 narrative with émojis 👍 fact: Ü");
}

#[test]
fn test_observation_embedding_text_many_facts() {
    let facts: Vec<String> = (0..100).map(|i| format!("fact-{i}")).collect();
    let obs = opencode_mem_core::Observation::builder(
        "obs-emb-many".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "T".to_owned(),
    )
    .narrative("N")
    .facts(facts)
    .build();

    let text = observation_embedding_text(&obs);
    assert!(text.starts_with("T N fact-0"));
    assert!(text.ends_with("fact-99"));
    // Verify all 100 facts are present
    for i in 0..100 {
        assert!(text.contains(&format!("fact-{i}")), "missing fact-{i} in embedding text");
    }
}

#[test]
fn test_observation_embedding_text_empty_title() {
    // Edge case: empty title. The builder allows it.
    let obs = opencode_mem_core::Observation::builder(
        "obs-emb-empty-title".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        String::new(), // empty title
    )
    .narrative("narrative")
    .facts(vec!["f".to_owned()])
    .build();

    let text = observation_embedding_text(&obs);
    assert_eq!(text, " narrative f");
}
