use super::test_fixtures::{create_pg_storage, make_observation, unique_id};
use opencode_mem_core::EMBEDDING_DIMENSION;
use opencode_mem_storage::traits::{EmbeddingStore, ObservationStore, SearchStore};

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
    if let Some(e) = embedding.get_mut(0) {
        *e = 1.0;
    }
    if let Some(e) = embedding.get_mut(1) {
        *e = 0.5;
    }
    if let Some(e) = embedding.get_mut(2) {
        *e = 0.25;
    }

    storage.store_embedding(&id, &embedding).await.unwrap();

    let without = storage
        .get_observations_without_embeddings(1000)
        .await
        .unwrap();
    let still_missing = without.iter().any(|o| *o.id == id);
    assert!(
        !still_missing,
        "Observation should no longer be in 'without embeddings' list"
    );

    let results = storage.semantic_search(&embedding, 10).await.unwrap();
    let found = results.iter().any(|r| *r.id == id);
    assert!(
        found,
        "Observation should be found via semantic search with matching vector"
    );
}
