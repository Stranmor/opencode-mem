use super::test_fixtures::{create_pg_storage, make_observation, unique_id};
use opencode_mem_storage::traits::{ObservationStore, SearchStore};

#[tokio::test]
#[ignore]
async fn pg_search_observations() {
    let storage = create_pg_storage().await;
    let id = unique_id();
    let project = unique_id();
    let title = format!("Xylophone integration marker {id}");
    let obs = make_observation(&id, "pg-test-session", &project, &title);
    storage
        .save_observation(&obs)
        .await
        .expect("save_observation failed");

    let results = storage.search("xylophone", 10).await.unwrap();
    let found = results.iter().any(|r| *r.id == id);
    assert!(
        found,
        "Observation should be found via FTS search for 'xylophone'"
    );
}
