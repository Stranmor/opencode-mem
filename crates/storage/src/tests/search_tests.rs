#![expect(clippy::unwrap_used, reason = "test code")]
#![expect(clippy::indexing_slicing, reason = "test code — asserts guard length")]

use super::{create_test_observation, create_test_storage};
use chrono::{TimeZone, Utc};
use opencode_mem_core::{NoiseLevel, ObservationType};

#[test]
fn test_search_with_filters() {
    let (storage, _temp_dir) = create_test_storage();

    assert!(storage.save_observation(&create_test_observation("obs-1", "project-a")).unwrap());
    assert!(storage.save_observation(&create_test_observation("obs-2", "project-b")).unwrap());

    let results =
        storage.search_with_filters(None, Some("project-a"), None, None, None, 10).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_get_context_for_project() {
    let (storage, _temp_dir) = create_test_storage();

    for i in 1..=5 {
        assert!(storage
            .save_observation(&create_test_observation(&format!("obs-{}", i), "my-project"))
            .unwrap());
    }

    let context = storage.get_context_for_project("my-project", 3).unwrap();
    assert_eq!(context.len(), 3);
}

#[test]
fn test_search_by_file() {
    let (storage, _temp_dir) = create_test_storage();

    assert!(storage.save_observation(&create_test_observation("obs-1", "project-a")).unwrap());

    let results = storage.search_by_file("file1.rs", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "obs-1");

    let no_results = storage.search_by_file("nonexistent.rs", 10).unwrap();
    assert!(no_results.is_empty());
}

#[test]
fn test_search_sessions_empty_query() {
    let (storage, _temp_dir) = create_test_storage();

    let results = storage.search_sessions("", 10).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_search_prompts_empty_query() {
    let (storage, _temp_dir) = create_test_storage();

    let results = storage.search_prompts("", 10).unwrap();
    assert!(results.is_empty());
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_search_with_date_range_filters() {
    let (storage, _temp_dir) = create_test_storage();

    let old_obs = opencode_mem_core::Observation::builder(
        "obs-old".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Old observation".to_owned(),
    )
    .project("proj")
    .created_at(Utc.with_ymd_and_hms(2025, 1, 15, 12, 0, 0).unwrap())
    .noise_level(NoiseLevel::Medium)
    .build();

    let new_obs = opencode_mem_core::Observation::builder(
        "obs-new".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "New observation".to_owned(),
    )
    .project("proj")
    .created_at(Utc.with_ymd_and_hms(2026, 6, 15, 12, 0, 0).unwrap())
    .noise_level(NoiseLevel::Medium)
    .build();

    assert!(storage.save_observation(&old_obs).unwrap());
    assert!(storage.save_observation(&new_obs).unwrap());

    // Filter with `from` — only the new observation should match
    let results =
        storage.search_with_filters(None, None, None, Some("2026-01-01"), None, 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "obs-new");

    // Filter with `to` — only the old observation should match
    let results =
        storage.search_with_filters(None, None, None, None, Some("2025-12-31"), 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "obs-old");

    // Both filters — range that includes only the old one
    let results = storage
        .search_with_filters(None, None, None, Some("2025-01-01"), Some("2025-12-31"), 10)
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "obs-old");

    // Both filters — range that includes neither
    let results = storage
        .search_with_filters(None, None, None, Some("2024-01-01"), Some("2024-12-31"), 10)
        .unwrap();
    assert!(results.is_empty());
}
