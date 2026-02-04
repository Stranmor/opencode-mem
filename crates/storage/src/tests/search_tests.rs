use super::{create_test_observation, create_test_storage};

#[test]
fn test_search_with_filters() {
    let (storage, _temp_dir) = create_test_storage();

    storage
        .save_observation(&create_test_observation("obs-1", "project-a"))
        .unwrap();
    storage
        .save_observation(&create_test_observation("obs-2", "project-b"))
        .unwrap();

    let results = storage
        .search_with_filters(None, Some("project-a"), None, 10)
        .unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_get_context_for_project() {
    let (storage, _temp_dir) = create_test_storage();

    for i in 1..=5 {
        storage
            .save_observation(&create_test_observation(
                &format!("obs-{}", i),
                "my-project",
            ))
            .unwrap();
    }

    let context = storage.get_context_for_project("my-project", 3).unwrap();
    assert_eq!(context.len(), 3);
}

#[test]
fn test_search_by_file() {
    let (storage, _temp_dir) = create_test_storage();

    storage
        .save_observation(&create_test_observation("obs-1", "project-a"))
        .unwrap();

    let results = storage.search_by_file("file1.rs", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "obs-1");

    let no_results = storage.search_by_file("nonexistent.rs", 10).unwrap();
    assert!(no_results.is_empty());
}

#[test]
fn test_search_sessions_empty_query() {
    let (storage, _temp_dir) = create_test_storage();

    let results = storage.search_sessions("", 10).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_search_prompts_empty_query() {
    let (storage, _temp_dir) = create_test_storage();

    let results = storage.search_prompts("", 10).unwrap();
    assert!(results.is_empty());
}
