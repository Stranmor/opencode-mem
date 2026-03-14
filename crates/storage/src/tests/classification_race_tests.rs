use opencode_mem_core::{Observation, ObservationType, NoiseLevel, ObservationMetadata};
use crate::pg_storage::PgStorage;
use crate::traits::ObservationStore;

#[tokio::test]
#[ignore]
async fn test_classification_corrupts_existing_data() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .unwrap();
    let storage = PgStorage::from_pool(pool);
    
    let id = uuid::Uuid::new_v4().to_string();
    let mut obs = Observation::builder(
        id.clone(),
        "manual".to_owned(),
        ObservationType::Bugfix, // User explicitly set Bugfix
        "Test observation".to_owned(),
    )
    .noise_level(NoiseLevel::Critical) // User explicitly set Critical
    .build();
    
    storage.save_observation(&obs).await.unwrap();
    
    // Background task generates metadata
    let mut metadata = ObservationMetadata::placeholder();
    metadata.observation_type = Some(ObservationType::Gotcha);
    metadata.noise_level = Some(NoiseLevel::Low);
    
    // Background task updates metadata
    storage.update_observation_metadata(&id, &metadata).await.unwrap();
    
    // Fetch and check
    let fetched = storage.get_by_id(&id).await.unwrap().unwrap();
    
    assert_eq!(fetched.observation_type, ObservationType::Bugfix, "Vulnerability: Background enrichment corrupted user's explicit observation_type!");
    assert_eq!(fetched.noise_level, NoiseLevel::Critical, "Vulnerability: Background enrichment corrupted user's explicit noise_level!");
    
    // Cleanup
    // (No cascade delete in this simple test, but good enough for demonstration)
}
