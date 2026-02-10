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
