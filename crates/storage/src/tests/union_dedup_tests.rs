//! Tests for `union_dedup` — pure function tests, no DB required.

#![allow(clippy::unwrap_used)]

use opencode_mem_core::union_dedup;

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
