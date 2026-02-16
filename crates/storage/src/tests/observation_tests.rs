#![expect(clippy::unwrap_used, reason = "test code")]

use super::{create_test_observation, create_test_storage};

#[test]
fn test_storage_new() {
    let (storage, _temp_dir) = create_test_storage();
    let stats = storage.get_stats().unwrap();
    assert_eq!(stats.observation_count, 0);
    assert_eq!(stats.session_count, 0);
}

#[test]
fn test_save_and_get_observation() {
    let (storage, _temp_dir) = create_test_storage();
    let obs = create_test_observation("obs-1", "test-project");

    assert!(storage.save_observation(&obs).unwrap());

    let retrieved = storage.get_by_id("obs-1").unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, "obs-1");
    assert_eq!(retrieved.title, "Test observation obs-1");
}

#[test]
fn test_get_recent() {
    let (storage, _temp_dir) = create_test_storage();

    for i in 1..=5 {
        let obs = create_test_observation(&format!("obs-{}", i), "test-project");
        assert!(storage.save_observation(&obs).unwrap());
    }

    let recent = storage.get_recent(3).unwrap();
    assert_eq!(recent.len(), 3);
}

#[test]
fn test_get_all_projects() {
    let (storage, _temp_dir) = create_test_storage();

    assert!(storage.save_observation(&create_test_observation("obs-1", "project-a")).unwrap());
    assert!(storage.save_observation(&create_test_observation("obs-2", "project-b")).unwrap());
    assert!(storage.save_observation(&create_test_observation("obs-3", "project-a")).unwrap());

    let projects = storage.get_all_projects().unwrap();
    assert_eq!(projects.len(), 2);
    assert!(projects.contains(&"project-a".to_string()));
    assert!(projects.contains(&"project-b".to_string()));
}

#[test]
fn test_get_stats() {
    let (storage, _temp_dir) = create_test_storage();

    assert!(storage.save_observation(&create_test_observation("obs-1", "project-a")).unwrap());
    storage.save_session(&super::create_test_session("session-1")).unwrap();

    let stats = storage.get_stats().unwrap();
    assert_eq!(stats.observation_count, 1);
    assert_eq!(stats.session_count, 1);
    assert_eq!(stats.project_count, 1);
}

#[test]
fn test_get_observations_paginated() {
    let (storage, _temp_dir) = create_test_storage();

    for i in 1..=10 {
        let obs = create_test_observation(&format!("obs-{}", i), "test-project");
        assert!(storage.save_observation(&obs).unwrap());
    }

    let page1 = storage.get_observations_paginated(0, 5, None).unwrap();
    assert_eq!(page1.items.len(), 5);
    assert_eq!(page1.total, 10);
    assert_eq!(page1.offset, 0);
    assert_eq!(page1.limit, 5);

    let page2 = storage.get_observations_paginated(5, 5, None).unwrap();
    assert_eq!(page2.items.len(), 5);
    assert_eq!(page2.offset, 5);
}

#[test]
#[expect(clippy::unwrap_used, reason = "test code")]
#[expect(clippy::indexing_slicing, reason = "test code — asserts guard length")]
fn test_store_embedding_overwrites_existing() {
    let (storage, _temp_dir) = create_test_storage();
    let obs = create_test_observation("obs-emb-1", "project-a");
    assert!(storage.save_observation(&obs).unwrap());

    // given: an observation with an existing embedding
    let embedding_v1: Vec<f32> = (0..384).map(|i: u16| f32::from(i) * 0.001).collect();
    storage.store_embedding("obs-emb-1", &embedding_v1).unwrap();
    let without = storage.get_observations_without_embeddings(100).unwrap();
    assert!(without.iter().all(|o| o.id != "obs-emb-1"));

    // when: overwriting with a different embedding
    let embedding_v2: Vec<f32> =
        (0..384_u16).map(|i| f32::from(384_u16.saturating_sub(i)) * 0.001).collect();
    storage.store_embedding("obs-emb-1", &embedding_v2).unwrap();

    // then: exactly one embedding exists (atomic replace, no duplicate)
    let without = storage.get_observations_without_embeddings(100).unwrap();
    assert!(without.iter().all(|o| o.id != "obs-emb-1"));

    // then: the new embedding is queryable via semantic search
    let results = storage.semantic_search(&embedding_v2, 10).unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].id, "obs-emb-1");
}

#[test]
fn test_get_observations_paginated_with_project_filter() {
    let (storage, _temp_dir) = create_test_storage();

    for i in 1..=5 {
        assert!(storage
            .save_observation(&create_test_observation(&format!("obs-a-{}", i), "project-a"))
            .unwrap());
    }
    for i in 1..=3 {
        assert!(storage
            .save_observation(&create_test_observation(&format!("obs-b-{}", i), "project-b"))
            .unwrap());
    }

    let result = storage.get_observations_paginated(0, 10, Some("project-a")).unwrap();
    assert_eq!(result.total, 5);
    assert_eq!(result.items.len(), 5);
}
