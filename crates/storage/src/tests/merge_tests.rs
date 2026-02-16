//! Tests for `merge_into_existing` — DB integration tests.

#![allow(clippy::unwrap_used)]

use super::{create_test_observation, create_test_storage};
use opencode_mem_core::{Concept, DiscoveryTokens, ObservationType, PromptNumber};

#[test]
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
fn test_merge_preserves_non_merged_fields() {
    // Regression guard: merge touches facts, keywords, files_read,
    // files_modified, narrative, created_at, noise_level, and subtitle.
    // All other fields must be preserved.
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
    assert_eq!(merged.subtitle.as_deref(), Some("different subtitle"), "subtitle picks longer");
    assert_eq!(merged.prompt_number, Some(PromptNumber(42)), "prompt_number must be preserved");
    assert_eq!(
        merged.discovery_tokens,
        Some(DiscoveryTokens(999)),
        "discovery_tokens must be preserved"
    );

    // Narrative was merged (newer is longer → wins):
    assert_eq!(merged.narrative.as_deref(), Some("longer narrative wins"));
}

#[test]
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

#[test]
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
