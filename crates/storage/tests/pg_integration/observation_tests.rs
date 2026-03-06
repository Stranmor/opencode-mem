use super::helpers::{create_pg_storage, make_observation, unique_id};
use opencode_mem_storage::traits::ObservationStore;

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
    assert_eq!(*fetched.id, id);
    assert_eq!(fetched.title, title);
    assert_eq!(fetched.project.as_deref(), Some(project.as_str()));
    assert_eq!(
        fetched.narrative.as_deref(),
        Some("Test narrative for integration")
    );
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

    let recent = storage.get_recent(2).await.unwrap();
    assert!(
        recent.len() >= 2,
        "Should return at least 2 recent observations"
    );
}
