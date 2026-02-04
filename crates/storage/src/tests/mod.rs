//! Test utilities and module declarations for storage tests

mod observation_tests;
mod queue_tests;
mod search_tests;
mod session_tests;

use crate::Storage;
use chrono::Utc;
use opencode_mem_core::{Observation, ObservationType, Session, SessionStatus};
use tempfile::TempDir;

pub fn create_test_storage() -> (Storage, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = Storage::new(&db_path).unwrap();
    (storage, temp_dir)
}

pub fn create_test_observation(id: &str, project: &str) -> Observation {
    Observation {
        id: id.to_string(),
        session_id: "test-session".to_string(),
        project: Some(project.to_string()),
        observation_type: ObservationType::Discovery,
        title: format!("Test observation {}", id),
        subtitle: Some("Test subtitle".to_string()),
        narrative: Some("Test narrative".to_string()),
        facts: vec!["fact1".to_string(), "fact2".to_string()],
        concepts: vec![],
        files_read: vec!["file1.rs".to_string()],
        files_modified: vec!["file2.rs".to_string()],
        keywords: vec!["test".to_string(), "keyword".to_string()],
        prompt_number: Some(1),
        discovery_tokens: Some(100),
        created_at: Utc::now(),
    }
}

pub fn create_test_session(id: &str) -> Session {
    Session {
        id: id.to_string(),
        content_session_id: format!("content-{}", id),
        memory_session_id: Some(format!("memory-{}", id)),
        project: "test-project".to_string(),
        user_prompt: Some("Test prompt".to_string()),
        started_at: Utc::now(),
        ended_at: None,
        status: SessionStatus::Active,
        prompt_counter: 0,
    }
}
