use super::test_fixtures::{create_pg_storage, make_observation, unique_id};
use opencode_mem_storage::traits::{ObservationStore, SearchStore};

#[tokio::test]
#[ignore]
async fn pg_search_observations() {
    let storage = create_pg_storage().await;
    let id = unique_id();
    let project = unique_id();
    let search_term = format!("xylophone-{id}");
    let title = format!("{search_term} integration marker");
    let obs = make_observation(&id, "pg-test-session", &project, &title);
    storage
        .save_observation(&obs)
        .await
        .expect("save_observation failed");

    let results = storage.search(&search_term, 10).await.unwrap();
    let found = results.iter().any(|r| r.id.0 == id);
    assert!(
        found,
        "Observation should be found via FTS search for 'xylophone'"
    );
}
