//! Test utilities and module declarations for storage tests.

use crate::Storage;
use chrono::Utc;
use opencode_mem_core::{NoiseLevel, Observation, ObservationType, Session, SessionStatus};
use tempfile::TempDir;

#[expect(dead_code, reason = "test utility function")]
#[expect(clippy::unwrap_used, reason = "test code")]
pub fn create_test_storage() -> (Storage, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = Storage::new(&db_path).unwrap();
    (storage, temp_dir)
}

#[expect(dead_code, reason = "test utility function")]
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

#[expect(dead_code, reason = "test utility function")]
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
