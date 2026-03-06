use super::test_fixtures::{create_pg_storage, make_session, unique_id};
use opencode_mem_core::SessionStatus;
use opencode_mem_storage::traits::SessionStore;

#[tokio::test]
#[ignore]
async fn pg_save_and_get_session() {
    let storage = create_pg_storage().await;
    let id = unique_id();
    let project = unique_id();
    let session = make_session(&id, &project);

    storage.save_session(&session).await.unwrap();

    let fetched = storage.get_session(&id).await.unwrap();
    assert!(fetched.is_some(), "Session should exist after save");
    let fetched = fetched.unwrap();
    assert_eq!(*fetched.id, id);
    assert_eq!(*fetched.project, project);
    assert_eq!(fetched.status, SessionStatus::Active);
    assert_eq!(*fetched.content_session_id, format!("content-{id}"));

    storage.delete_session(&id).await.unwrap();
}

#[tokio::test]
#[ignore]
async fn pg_session_status_update() {
    let storage = create_pg_storage().await;
    let id = unique_id();
    let project = unique_id();
    let session = make_session(&id, &project);

    storage.save_session(&session).await.unwrap();

    storage
        .update_session_status(&id, SessionStatus::Completed)
        .await
        .unwrap();

    let fetched = storage.get_session(&id).await.unwrap().unwrap();
    assert_eq!(fetched.status, SessionStatus::Completed);
    assert!(
        fetched.ended_at.is_some(),
        "ended_at should be set when status is non-Active"
    );

    storage.delete_session(&id).await.unwrap();
}
