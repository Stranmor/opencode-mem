use super::{create_test_session, create_test_storage};
use opencode_mem_core::SessionStatus;

#[test]
fn test_save_and_get_session() {
    let (storage, _temp_dir) = create_test_storage();
    let session = create_test_session("session-1");

    storage.save_session(&session).unwrap();

    let retrieved = storage.get_session("session-1").unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, "session-1");
    assert_eq!(retrieved.status, SessionStatus::Active);
}

#[test]
fn test_update_session_status() {
    let (storage, _temp_dir) = create_test_storage();
    let session = create_test_session("session-2");
    storage.save_session(&session).unwrap();

    storage.update_session_status("session-2", SessionStatus::Completed).unwrap();

    let retrieved = storage.get_session("session-2").unwrap().unwrap();
    assert_eq!(retrieved.status, SessionStatus::Completed);
    assert!(retrieved.ended_at.is_some());
}
