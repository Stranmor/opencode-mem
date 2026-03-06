use super::test_fixtures::{create_pg_storage, make_observation, make_session, unique_id};
use opencode_mem_storage::traits::{ObservationStore, SessionStore, StatsStore};

#[tokio::test]
#[ignore]
async fn pg_stats() {
    let storage = create_pg_storage().await;

    let obs_id = unique_id();
    let project = unique_id();
    let obs = make_observation(
        &obs_id,
        "pg-test-session",
        &project,
        &format!("Stats test {obs_id}"),
    );
    storage.save_observation(&obs).await.unwrap();

    let sess_id = unique_id();
    let session = make_session(&sess_id, &project);
    storage.save_session(&session).await.unwrap();

    let stats = storage.get_stats().await.unwrap();
    assert!(
        stats.observation_count > 0,
        "Should have at least 1 observation"
    );
    assert!(stats.session_count > 0, "Should have at least 1 session");

    storage.delete_session(&sess_id).await.unwrap();
}
