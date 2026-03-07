use super::test_fixtures::{create_pg_storage, unique_id};
use opencode_mem_storage::traits::PendingQueueStore;

#[tokio::test]
#[ignore]
async fn pg_pending_queue_lifecycle() {
    let storage = create_pg_storage().await;
    let session = unique_id();

    let msg_id = storage
        .queue_message(
            &session,
            Some("call-123"),
            Some("test_tool"),
            Some(r#"{"key":"value"}"#),
            Some("tool response"),
            Some("pg-test-project"),
        )
        .await
        .unwrap();
    assert!(msg_id > 0, "queue_message should return positive ID");

    let claimed = storage.claim_pending_messages(10, 300).await.unwrap();
    let ours = claimed.iter().find(|m| m.id == msg_id);
    assert!(ours.is_some(), "Our message should be claimed");

    storage.complete_message(msg_id).await.unwrap();

    let all = storage.get_all_pending_messages(100).await.unwrap();
    let still_there = all.iter().any(|m| m.id == msg_id);
    assert!(!still_there, "Completed message should be deleted");
}
