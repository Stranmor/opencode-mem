//! Integration tests for knowledge trigram dedup.
//! Requires PostgreSQL with pg_trgm extension.
//! Run with: `cargo test --workspace -- --ignored knowledge_trigram`

#![allow(clippy::unwrap_used, reason = "Unwraps are safe in tests")]

use opencode_mem_core::{
    KnowledgeInput, KnowledgeType, PG_POOL_ACQUIRE_TIMEOUT_SECS, PG_POOL_IDLE_TIMEOUT_SECS,
    PG_POOL_MAX_CONNECTIONS,
};
use sqlx::postgres::PgPoolOptions;

use crate::pg_storage::PgStorage;
use crate::traits::KnowledgeStore;

async fn test_storage() -> PgStorage {
    let url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for integration tests");
    let pool = PgPoolOptions::new()
        .max_connections(PG_POOL_MAX_CONNECTIONS)
        .acquire_timeout(std::time::Duration::from_secs(PG_POOL_ACQUIRE_TIMEOUT_SECS))
        .idle_timeout(std::time::Duration::from_secs(PG_POOL_IDLE_TIMEOUT_SECS))
        .connect(&url)
        .await
        .unwrap();
    PgStorage::from_pool(pool)
}

fn knowledge_input(title: &str, description: &str, source_project: Option<&str>) -> KnowledgeInput {
    KnowledgeInput::new(
        KnowledgeType::Gotcha,
        title.to_owned(),
        description.to_owned(),
        None,
        vec!["trigger1".to_owned()],
        source_project.map(ToOwned::to_owned),
        None,
    )
}

#[tokio::test]
#[ignore = "requires PostgreSQL with pg_trgm"]
async fn trigram_dedup_merges_similar_titles() {
    let storage = test_storage().await;
    let unique_suffix = uuid::Uuid::new_v4().to_string();

    let original_title = format!("Telegram MTProto channel publishing method [{unique_suffix}]");
    let similar_title = format!("Telegram MTProto channel publishing approach [{unique_suffix}]");

    let first = storage
        .save_knowledge(knowledge_input(
            &original_title,
            "Original description",
            Some("proj-a"),
        ))
        .await
        .unwrap();

    let second = storage
        .save_knowledge(knowledge_input(
            &similar_title,
            "Updated description",
            Some("proj-b"),
        ))
        .await
        .unwrap();

    // Should merge into the first entry (same ID)
    assert_eq!(
        second.id, first.id,
        "similar title should merge into existing entry"
    );
    // Should keep the original title
    assert_eq!(second.title, original_title);
    // Description should be updated
    assert_eq!(second.description, "Updated description");
    // Source projects should be merged
    assert!(second.source_projects.contains(&"proj-a".to_owned()));
    assert!(second.source_projects.contains(&"proj-b".to_owned()));

    // Cleanup
    storage.delete_knowledge(&first.id).await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL with pg_trgm"]
async fn trigram_dedup_creates_for_different_titles() {
    let storage = test_storage().await;
    let unique_suffix_a = uuid::Uuid::new_v4().to_string();
    let unique_suffix_b = uuid::Uuid::new_v4().to_string();

    let title_a = format!("Rust async patterns and tokio setup [{unique_suffix_a}]");
    let title_b = format!("PostgreSQL connection pooling configuration [{unique_suffix_b}]");

    let first = storage
        .save_knowledge(knowledge_input(&title_a, "Description A", Some("proj-a")))
        .await
        .unwrap();

    let second = storage
        .save_knowledge(knowledge_input(&title_b, "Description B", Some("proj-b")))
        .await
        .unwrap();

    assert_ne!(
        first.id, second.id,
        "different titles should create separate entries"
    );

    let _ = storage.delete_knowledge(&first.id).await;
    let _ = storage.delete_knowledge(&second.id).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL with pg_trgm"]
async fn trigram_dedup_exact_match_still_works() {
    let storage = test_storage().await;
    let unique_suffix = uuid::Uuid::new_v4().to_string();

    let title = format!("Exact title match test [{unique_suffix}]");

    let first = storage
        .save_knowledge(knowledge_input(&title, "First description", Some("proj-a")))
        .await
        .unwrap();

    let second = storage
        .save_knowledge(knowledge_input(
            &title,
            "Second description",
            Some("proj-b"),
        ))
        .await
        .unwrap();

    // Exact match uses the fast path (existing upsert behavior)
    assert_eq!(
        second.id, first.id,
        "exact title should merge via fast path"
    );
    assert!(second.source_projects.contains(&"proj-a".to_owned()));
    assert!(second.source_projects.contains(&"proj-b".to_owned()));

    // Cleanup
    storage.delete_knowledge(&first.id).await.unwrap();
}

#[tokio::test]
#[ignore = "requires PostgreSQL with pg_trgm"]
async fn trigram_dedup_merges_triggers() {
    let storage = test_storage().await;
    let unique_suffix = uuid::Uuid::new_v4().to_string();

    let original_title =
        format!("sshd requires absolute path for binary execution [{unique_suffix}]");
    let similar_title =
        format!("sshd requires absolute path for daemon execution [{unique_suffix}]");

    let mut input1 = knowledge_input(&original_title, "Desc 1", None);
    input1.triggers = vec!["ssh".to_owned(), "sshd".to_owned()];

    let mut input2 = knowledge_input(&similar_title, "Desc 2", None);
    input2.triggers = vec!["sshd".to_owned(), "daemon".to_owned()];

    let first = storage.save_knowledge(input1).await.unwrap();
    let second = storage.save_knowledge(input2).await.unwrap();

    assert_eq!(second.id, first.id);
    assert!(second.triggers.contains(&"ssh".to_owned()));
    assert!(second.triggers.contains(&"sshd".to_owned()));
    assert!(second.triggers.contains(&"daemon".to_owned()));

    // Cleanup
    storage.delete_knowledge(&first.id).await.unwrap();
}
