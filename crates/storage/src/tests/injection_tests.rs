use super::create_test_storage;

#[test]
fn save_and_get_injected_observations() {
    let (storage, _dir) = create_test_storage();
    let ids = vec!["obs-1".to_owned(), "obs-2".to_owned(), "obs-3".to_owned()];
    storage.save_injected_observations("session-A", &ids).unwrap();

    let retrieved = storage.get_injected_observation_ids("session-A").unwrap();
    assert_eq!(retrieved.len(), 3);
    assert!(retrieved.contains(&"obs-1".to_owned()));
    assert!(retrieved.contains(&"obs-2".to_owned()));
    assert!(retrieved.contains(&"obs-3".to_owned()));
}

#[test]
fn empty_ids_is_noop() {
    let (storage, _dir) = create_test_storage();
    storage.save_injected_observations("session-A", &[]).unwrap();
    let retrieved = storage.get_injected_observation_ids("session-A").unwrap();
    assert!(retrieved.is_empty());
}

#[test]
fn different_sessions_isolated() {
    let (storage, _dir) = create_test_storage();
    storage
        .save_injected_observations("session-A", &["obs-a1".to_owned(), "obs-a2".to_owned()])
        .unwrap();
    storage.save_injected_observations("session-B", &["obs-b1".to_owned()]).unwrap();

    let a_ids = storage.get_injected_observation_ids("session-A").unwrap();
    let b_ids = storage.get_injected_observation_ids("session-B").unwrap();

    assert_eq!(a_ids.len(), 2);
    assert!(a_ids.contains(&"obs-a1".to_owned()));
    assert!(a_ids.contains(&"obs-a2".to_owned()));
    assert!(!a_ids.contains(&"obs-b1".to_owned()));

    assert_eq!(b_ids.len(), 1);
    assert!(b_ids.contains(&"obs-b1".to_owned()));
}

#[test]
fn duplicate_insert_ignored() {
    let (storage, _dir) = create_test_storage();
    storage.save_injected_observations("session-A", &["obs-1".to_owned()]).unwrap();
    storage.save_injected_observations("session-A", &["obs-1".to_owned()]).unwrap();

    let retrieved = storage.get_injected_observation_ids("session-A").unwrap();
    assert_eq!(retrieved.len(), 1);
}

#[test]
fn cleanup_old_injections_runs_without_error() {
    let (storage, _dir) = create_test_storage();
    storage.save_injected_observations("session-A", &["obs-1".to_owned()]).unwrap();
    let deleted = storage.cleanup_old_injections(24).unwrap();
    assert_eq!(deleted, 0);
}

#[test]
fn get_embeddings_returns_empty_for_no_ids() {
    let (storage, _dir) = create_test_storage();
    let result = storage.get_embeddings_for_ids(&[]).unwrap();
    assert!(result.is_empty());
}

#[test]
fn get_embeddings_returns_empty_for_nonexistent_ids() {
    let (storage, _dir) = create_test_storage();
    let ids = vec!["nonexistent-1".to_owned(), "nonexistent-2".to_owned()];
    let result = storage.get_embeddings_for_ids(&ids).unwrap();
    assert!(result.is_empty());
}
