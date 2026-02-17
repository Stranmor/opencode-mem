//! Test utilities and module declarations for storage tests.

use crate::Storage;
use chrono::Utc;
use opencode_mem_core::{NoiseLevel, Observation, ObservationType, Session, SessionStatus};
use tempfile::TempDir;

#[expect(clippy::unwrap_used, reason = "test code")]
pub fn create_test_storage() -> (Storage, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = Storage::new(&db_path).unwrap();
    (storage, temp_dir)
}

pub fn create_test_observation(id: &str, project: &str) -> Observation {
    Observation::builder(
        id.to_owned(),
        "test-session".to_owned(),
        ObservationType::Discovery,
        format!("Test observation {id}"),
    )
    .project(project)
    .subtitle("Test subtitle")
    .narrative("Test narrative")
    .facts(vec!["fact1".to_owned(), "fact2".to_owned()])
    .files_read(vec!["file1.rs".to_owned()])
    .files_modified(vec!["file2.rs".to_owned()])
    .keywords(vec!["test".to_owned(), "keyword".to_owned()])
    .prompt_number(1)
    .discovery_tokens(100)
    .noise_level(NoiseLevel::Medium)
    .build()
}

pub fn create_test_session(id: &str) -> Session {
    Session::new(
        id.to_owned(),
        format!("content-{id}"),
        Some(format!("memory-{id}")),
        "test-project".to_owned(),
        Some("Test prompt".to_owned()),
        Utc::now(),
        None,
        SessionStatus::Active,
        0,
    )
}

mod embedding_text_tests;
mod find_similar_tests;
mod injection_tests;
mod merge_tests;
mod observation_tests;
mod search_tests;
mod union_dedup_tests;

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn save_observation_dedup_via_unique_index() {
    let (storage, _temp_dir) = create_test_storage();
    let obs1 = Observation::builder(
        "obs-1".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "  IsolationManager uses HRW hashing for deterministic proxy assignment  ".to_owned(),
    )
    .build();

    assert!(storage.save_observation(&obs1).unwrap());

    let obs2 = Observation::builder(
        "obs-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "isolationmanager uses hrw hashing for deterministic proxy assignment".to_owned(),
    )
    .build();

    assert!(!storage.save_observation(&obs2).unwrap());

    let count = storage.get_session_observation_count("session-1").unwrap();
    assert_eq!(count, 1);
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_save_observation_duplicate_returns_false() {
    let (storage, _temp_dir) = create_test_storage();
    let obs = Observation::builder(
        "obs-dup-1".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Duplicate test observation".to_owned(),
    )
    .build();

    // given: observation saved successfully
    assert!(storage.save_observation(&obs).unwrap());

    // when: saving the same observation again (same id)
    let second_insert = storage.save_observation(&obs).unwrap();

    // then: returns false (INSERT OR IGNORE skipped the duplicate)
    assert!(!second_insert);

    let count = storage.get_session_observation_count("session-1").unwrap();
    assert_eq!(count, 1);
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_save_observation_different_id_same_title_returns_false() {
    let (storage, _temp_dir) = create_test_storage();

    let obs1 = Observation::builder(
        "obs-title-dup-1".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "  Shared Title With Extra Whitespace  ".to_owned(),
    )
    .build();

    let obs2 = Observation::builder(
        "obs-title-dup-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "shared title with extra whitespace".to_owned(),
    )
    .build();

    // given: first observation inserted
    assert!(storage.save_observation(&obs1).unwrap());

    // when: second observation with different id but same normalized title
    let second_insert = storage.save_observation(&obs2).unwrap();

    // then: dedup rejects via title_normalized UNIQUE constraint
    assert!(!second_insert);

    let count = storage.get_session_observation_count("session-1").unwrap();
    assert_eq!(count, 1);
}
