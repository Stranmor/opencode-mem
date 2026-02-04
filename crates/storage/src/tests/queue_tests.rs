use super::create_test_storage;

#[test]
fn test_queue_message() {
    let (storage, _temp_dir) = create_test_storage();

    let id = storage
        .queue_message(
            "session-1",
            Some("read"),
            Some(r#"{"path": "/foo"}"#),
            Some("file contents"),
        )
        .unwrap();
    assert!(id > 0);

    let count = storage.get_pending_count().unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_claim_pending_messages() {
    let (storage, _temp_dir) = create_test_storage();

    storage.queue_message("s1", Some("tool1"), None, None).unwrap();
    storage.queue_message("s2", Some("tool2"), None, None).unwrap();
    storage.queue_message("s3", Some("tool3"), None, None).unwrap();

    let claimed = storage.claim_pending_messages(2, 300).unwrap();
    assert_eq!(claimed.len(), 2);
    assert_eq!(claimed[0].tool_name, Some("tool1".to_string()));
    assert_eq!(claimed[1].tool_name, Some("tool2".to_string()));

    let pending = storage.get_pending_count().unwrap();
    assert_eq!(pending, 1);
}

#[test]
fn test_complete_message() {
    let (storage, _temp_dir) = create_test_storage();

    let id = storage.queue_message("s1", Some("tool"), None, None).unwrap();
    let _claimed = storage.claim_pending_messages(1, 300).unwrap();

    storage.complete_message(id).unwrap();

    let pending = storage.get_pending_count().unwrap();
    assert_eq!(pending, 0);
}

#[test]
fn test_fail_message_with_retry() {
    let (storage, _temp_dir) = create_test_storage();

    let id = storage.queue_message("s1", Some("tool"), None, None).unwrap();
    let _claimed = storage.claim_pending_messages(1, 300).unwrap();

    storage.fail_message(id, true).unwrap();
    let pending = storage.get_pending_count().unwrap();
    assert_eq!(pending, 1);

    let _claimed = storage.claim_pending_messages(1, 300).unwrap();
    storage.fail_message(id, true).unwrap();
    let pending = storage.get_pending_count().unwrap();
    assert_eq!(pending, 1);

    let _claimed = storage.claim_pending_messages(1, 300).unwrap();
    storage.fail_message(id, true).unwrap();
    let pending = storage.get_pending_count().unwrap();
    assert_eq!(pending, 0);

    let failed = storage.get_failed_messages(10).unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].retry_count, 3);
}

#[test]
fn test_release_stale_messages() {
    let (storage, _temp_dir) = create_test_storage();

    storage.queue_message("s1", Some("tool"), None, None).unwrap();
    let _claimed = storage.claim_pending_messages(1, 300).unwrap();

    let released = storage.release_stale_messages(0).unwrap();
    assert_eq!(released, 1);

    let pending = storage.get_pending_count().unwrap();
    assert_eq!(pending, 1);
}
