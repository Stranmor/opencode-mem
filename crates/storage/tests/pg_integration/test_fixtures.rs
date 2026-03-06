use chrono::Utc;
use opencode_mem_core::{
    ContentSessionId, NoiseLevel, Observation, ObservationType, ProjectId, Session, SessionId,
    SessionStatus,
};
use opencode_mem_storage::PgStorage;
use uuid::Uuid;

pub async fn create_pg_storage() -> PgStorage {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set for PgStorage integration tests");
    PgStorage::new(&url)
        .await
        .expect("Failed to connect to PostgreSQL")
}

pub fn unique_id() -> String {
    format!("test-{}", Uuid::new_v4())
}

pub fn make_observation(id: &str, session_id: &str, project: &str, title: &str) -> Observation {
    Observation::builder(
        id.to_owned(),
        session_id.to_owned(),
        ObservationType::Discovery,
        title.to_owned(),
    )
    .project(project)
    .subtitle("Test subtitle")
    .narrative("Test narrative for integration")
    .facts(vec!["fact1".to_owned(), "fact2".to_owned()])
    .files_read(vec!["file1.rs".to_owned()])
    .files_modified(vec!["file2.rs".to_owned()])
    .keywords(vec!["integration".to_owned(), "test".to_owned()])
    .prompt_number(1)
    .discovery_tokens(100)
    .noise_level(NoiseLevel::Medium)
    .build()
}

pub fn make_session(id: &str, project: &str) -> Session {
    Session::new(
        SessionId::from(id),
        ContentSessionId::from(format!("content-{id}")),
        Some(format!("memory-{id}")),
        ProjectId::from(project),
        Some("Test prompt".to_owned()),
        Utc::now(),
        None,
        SessionStatus::Active,
        0,
    )
}
