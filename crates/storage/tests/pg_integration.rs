//! Integration tests for PgStorage.
//! Run with: DATABASE_URL=... cargo test -p opencode-mem-storage --features postgres -- --ignored pg_

#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, reason = "integration test code")]

use chrono::Utc;
use opencode_mem_core::{
    KnowledgeInput, KnowledgeType, NoiseLevel, Observation, ObservationType, Session,
    SessionStatus, EMBEDDING_DIMENSION,
};
use opencode_mem_storage::traits::{
    EmbeddingStore, KnowledgeStore, ObservationStore, PendingQueueStore, SearchStore, SessionStore,
    StatsStore,
};
use opencode_mem_storage::PgStorage;
use uuid::Uuid;

async fn create_pg_storage() -> PgStorage {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set for PgStorage integration tests");
    PgStorage::new(&url).await.expect("Failed to connect to PostgreSQL")
}

fn unique_id() -> String {
    format!("test-{}", Uuid::new_v4())
}

fn make_observation(id: &str, session_id: &str, project: &str, title: &str) -> Observation {
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

fn make_session(id: &str, project: &str) -> Session {
    Session::new(
        id.to_owned(),
        format!("content-{id}"),
        Some(format!("memory-{id}")),
        project.to_owned(),
        Some("Test prompt".to_owned()),
        Utc::now(),
        None,
        SessionStatus::Active,
        0,
    )
}

// ── Observation Tests ────────────────────────────────────────────

#[tokio::test]
#[ignore]
async fn pg_save_and_get_observation() {
    let storage = create_pg_storage().await;
    let id = unique_id();
    let project = unique_id();
    let title = format!("Save-get test {id}");
    let obs = make_observation(&id, "pg-test-session", &project, &title);

    let inserted = storage.save_observation(&obs).await.unwrap();
    assert!(inserted, "First insert should return true");

    let fetched = storage.get_by_id(&id).await.unwrap();
    assert!(fetched.is_some(), "Observation should exist after save");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.id, id);
    assert_eq!(fetched.title, title);
    assert_eq!(fetched.project.as_deref(), Some(project.as_str()));
    assert_eq!(fetched.narrative.as_deref(), Some("Test narrative for integration"));
    assert_eq!(fetched.facts, vec!["fact1", "fact2"]);
    assert_eq!(fetched.keywords, vec!["integration", "test"]);
}

#[tokio::test]
#[ignore]
async fn pg_observation_dedup() {
    let storage = create_pg_storage().await;
    let id = unique_id();
    let project = unique_id();
    let title = format!("Dedup test {id}");
    let obs = make_observation(&id, "pg-test-session", &project, &title);

    let first = storage.save_observation(&obs).await.unwrap();
    assert!(first, "First insert should succeed");

    // Same ID → ON CONFLICT (id) DO NOTHING → returns false
    let second = storage.save_observation(&obs).await.unwrap();
    assert!(!second, "Second insert with same ID should return false");
}

#[tokio::test]
#[ignore]
async fn pg_get_recent_observations() {
    let storage = create_pg_storage().await;
    let project = unique_id();
    let session = unique_id();

    for i in 0..3 {
        let id = unique_id();
        let title = format!("Recent test {i} {id}");
        let obs = make_observation(&id, &session, &project, &title);
        storage.save_observation(&obs).await.unwrap();
    }

    // get_recent returns across ALL observations, not project-filtered,
    // so we just verify it returns at least some results.
    let recent = storage.get_recent(2).await.unwrap();
    assert!(recent.len() >= 2, "Should return at least 2 recent observations");
}

// ── Session Tests ────────────────────────────────────────────────

#[tokio::test]
#[ignore]
async fn pg_save_and_get_session() {
    let storage = create_pg_storage().await;
    let id = unique_id();
    let project = unique_id();
    let session = make_session(&id, &project);

    storage.save_session(&session).await.unwrap();

    let fetched = storage.get_session(&id).await.unwrap();
    assert!(fetched.is_some(), "Session should exist after save");
    let fetched = fetched.unwrap();
    assert_eq!(fetched.id, id);
    assert_eq!(fetched.project, project);
    assert_eq!(fetched.status, SessionStatus::Active);
    assert_eq!(fetched.content_session_id, format!("content-{id}"));

    // Cleanup
    storage.delete_session(&id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn pg_session_status_update() {
    let storage = create_pg_storage().await;
    let id = unique_id();
    let project = unique_id();
    let session = make_session(&id, &project);

    storage.save_session(&session).await.unwrap();

    storage.update_session_status(&id, SessionStatus::Completed).await.unwrap();

    let fetched = storage.get_session(&id).await.unwrap().unwrap();
    assert_eq!(fetched.status, SessionStatus::Completed);
    assert!(fetched.ended_at.is_some(), "ended_at should be set when status is non-Active");

    // Cleanup
    storage.delete_session(&id).await.unwrap();
}

// ── Knowledge Tests ──────────────────────────────────────────────

#[tokio::test]
#[ignore]
async fn pg_save_and_search_knowledge() {
    let storage = create_pg_storage().await;
    let tag = unique_id();
    let title = format!("Knowledge integration {tag}");

    let input = KnowledgeInput::new(
        KnowledgeType::Pattern,
        title.clone(),
        format!("Description for pattern {tag}"),
        Some("Step-by-step instructions".to_owned()),
        vec!["trigger-a".to_owned(), "trigger-b".to_owned()],
        Some("pg-test-project".to_owned()),
        Some("obs-ref".to_owned()),
    );

    let saved = storage.save_knowledge(input).await.unwrap();
    assert_eq!(saved.title, title);
    assert_eq!(saved.knowledge_type, KnowledgeType::Pattern);

    // Search by a word from the title
    let results = storage.search_knowledge("integration", 10).await.unwrap();
    let found = results.iter().any(|r| r.knowledge.id == saved.id);
    assert!(found, "Saved knowledge should be found via search");

    // Cleanup
    storage.delete_knowledge(&saved.id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn pg_knowledge_dedup() {
    let storage = create_pg_storage().await;
    let tag = unique_id();
    let title = format!("Dedup knowledge {tag}");

    let input1 = KnowledgeInput::new(
        KnowledgeType::Pattern,
        title.clone(),
        "First description".to_owned(),
        None,
        vec!["trigger1".to_owned()],
        Some("project-a".to_owned()),
        None,
    );
    let saved1 = storage.save_knowledge(input1).await.unwrap();

    let input2 = KnowledgeInput::new(
        KnowledgeType::Pattern,
        title.clone(),
        "Second description".to_owned(),
        None,
        vec!["trigger2".to_owned()],
        Some("project-b".to_owned()),
        None,
    );
    let saved2 = storage.save_knowledge(input2).await.unwrap();

    // Same ID = upsert by title match
    assert_eq!(saved1.id, saved2.id, "Same title should reuse the same ID");

    // Triggers should be merged
    let fetched = storage.get_knowledge(&saved2.id).await.unwrap().unwrap();
    assert!(fetched.triggers.contains(&"trigger1".to_owned()), "First trigger should be preserved");
    assert!(fetched.triggers.contains(&"trigger2".to_owned()), "Second trigger should be merged");

    // Description updated to latest
    assert_eq!(fetched.description, "Second description");

    // Cleanup
    storage.delete_knowledge(&saved1.id).await.unwrap();
}

// ── Pending Queue Tests ──────────────────────────────────────────

#[tokio::test]
#[ignore]
async fn pg_pending_queue_lifecycle() {
    let storage = create_pg_storage().await;
    let session = unique_id();

    let msg_id = storage
        .queue_message(
            &session,
            Some("test_tool"),
            Some(r#"{"key":"value"}"#),
            Some("tool response"),
            Some("pg-test-project"),
        )
        .await
        .unwrap();
    assert!(msg_id > 0, "queue_message should return positive ID");

    // Claim it
    let claimed = storage.claim_pending_messages(10, 300).await.unwrap();
    let ours = claimed.iter().find(|m| m.id == msg_id);
    assert!(ours.is_some(), "Our message should be claimed");

    // Complete it
    storage.complete_message(msg_id).await.unwrap();

    // Verify it's gone (complete_message deletes in PgStorage)
    let all = storage.get_all_pending_messages(100).await.unwrap();
    let still_there = all.iter().any(|m| m.id == msg_id);
    assert!(!still_there, "Completed message should be deleted");
}

// ── Search Tests ─────────────────────────────────────────────────

#[tokio::test]
#[ignore]
async fn pg_search_observations() {
    let storage = create_pg_storage().await;
    let id = unique_id();
    let project = unique_id();
    // Use a distinctive word for FTS matching
    let title = format!("Xylophone integration marker {id}");
    let obs = make_observation(&id, "pg-test-session", &project, &title);
    storage.save_observation(&obs).await.unwrap();

    // FTS search for the distinctive word
    let results = storage.search("xylophone", 10).await.unwrap();
    let found = results.iter().any(|r| r.id == id);
    assert!(found, "Observation should be found via FTS search for 'xylophone'");
}

// ── Stats Tests ──────────────────────────────────────────────────

#[tokio::test]
#[ignore]
async fn pg_stats() {
    let storage = create_pg_storage().await;

    // Insert at least one observation and session so counts are > 0
    let obs_id = unique_id();
    let project = unique_id();
    let obs =
        make_observation(&obs_id, "pg-test-session", &project, &format!("Stats test {obs_id}"));
    storage.save_observation(&obs).await.unwrap();

    let sess_id = unique_id();
    let session = make_session(&sess_id, &project);
    storage.save_session(&session).await.unwrap();

    let stats = storage.get_stats().await.unwrap();
    assert!(stats.observation_count > 0, "Should have at least 1 observation");
    assert!(stats.session_count > 0, "Should have at least 1 session");

    // Cleanup session (observations lack a delete API, but they use unique IDs so no interference)
    storage.delete_session(&sess_id).await.unwrap();
}

// ── Embedding Tests ──────────────────────────────────────────────

#[tokio::test]
#[ignore]
async fn pg_store_and_search_embedding() {
    let storage = create_pg_storage().await;
    let id = unique_id();
    let project = unique_id();
    let title = format!("Embedding test {id}");
    let obs = make_observation(&id, "pg-test-session", &project, &title);
    storage.save_observation(&obs).await.unwrap();

    let mut embedding = vec![0.0_f32; EMBEDDING_DIMENSION];
    embedding[0] = 1.0;
    embedding[1] = 0.5;
    embedding[2] = 0.25;

    storage.store_embedding(&id, &embedding).await.unwrap();

    // Verify observation no longer appears in "without embeddings"
    let without = storage.get_observations_without_embeddings(1000).await.unwrap();
    let still_missing = without.iter().any(|o| o.id == id);
    assert!(!still_missing, "Observation should no longer be in 'without embeddings' list");

    // Semantic search with same vector should find it
    let results = storage.semantic_search(&embedding, 10).await.unwrap();
    let found = results.iter().any(|r| r.id == id);
    assert!(found, "Observation should be found via semantic search with matching vector");
}
