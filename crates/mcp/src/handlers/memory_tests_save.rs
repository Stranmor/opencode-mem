use super::*;

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn test_save_memory_missing_text() {
    let backend = setup_storage().await;
    let obs_service = setup_observation_service(backend);
    let pending_writes = PendingWriteQueue::new();
    let args = json!({});
    let result = handle_save_memory(&obs_service, &pending_writes, &args).await;

    assert_eq!(result["isError"].as_bool(), Some(true));
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("text is required")
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn test_save_memory_empty_text() {
    let backend = setup_storage().await;
    let obs_service = setup_observation_service(backend);
    let pending_writes = PendingWriteQueue::new();
    let args = json!({ "text": "  " });
    let result = handle_save_memory(&obs_service, &pending_writes, &args).await;

    assert_eq!(result["isError"].as_bool(), Some(true));
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("must not be empty")
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn test_save_memory_with_title() {
    let backend = setup_storage().await;
    let obs_service = setup_observation_service(backend);
    let pending_writes = PendingWriteQueue::new();
    let args = json!({
        "text": "some narrative",
        "title": "custom title"
    });
    let result = handle_save_memory(&obs_service, &pending_writes, &args).await;

    assert!(result.get("isError").is_none());
    let obs_json = result["content"][0]["text"].as_str().unwrap();
    let obs: Observation = serde_json::from_str(obs_json).unwrap();
    assert_eq!(obs.title, "custom title");
    assert_eq!(obs.narrative.as_deref(), Some("some narrative"));
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn test_save_memory_without_title() {
    let backend = setup_storage().await;
    let obs_service = setup_observation_service(backend);
    let pending_writes = PendingWriteQueue::new();
    let long_text = "A very long text that should be truncated for the title because it is more than fifty characters long.";
    let args = json!({
        "text": long_text
    });
    let result = handle_save_memory(&obs_service, &pending_writes, &args).await;

    assert!(result.get("isError").is_none());
    let obs_json = result["content"][0]["text"].as_str().unwrap();
    let obs: Observation = serde_json::from_str(obs_json).unwrap();
    assert_eq!(obs.title.chars().count(), 50);
    assert!(long_text.starts_with(&obs.title));
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn test_save_memory_with_project() {
    let backend = setup_storage().await;
    let obs_service = setup_observation_service(backend);
    let pending_writes = PendingWriteQueue::new();
    let args = json!({
        "text": "narrative",
        "project": "test-project"
    });
    let result = handle_save_memory(&obs_service, &pending_writes, &args).await;

    assert!(result.get("isError").is_none());
    let obs_json = result["content"][0]["text"].as_str().unwrap();
    let obs: Observation = serde_json::from_str(obs_json).unwrap();
    assert_eq!(obs.project.as_deref(), Some("test-project"));
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn test_save_memory_success_returns_observation() {
    let backend = setup_storage().await;
    let obs_service = setup_observation_service(backend);
    let pending_writes = PendingWriteQueue::new();
    let args = json!({
        "text": "success test"
    });
    let result = handle_save_memory(&obs_service, &pending_writes, &args).await;

    assert!(result.get("isError").is_none());
    let content = &result["content"][0];
    assert_eq!(content["type"], "text");
    let obs_json = content["text"].as_str().unwrap();
    let _: Observation =
        serde_json::from_str(obs_json).expect("Should return valid Observation JSON");
}
