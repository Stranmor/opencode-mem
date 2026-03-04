use super::*;
use opencode_mem_core::{Observation, ObservationType};
use opencode_mem_service::{PendingWriteQueue, SearchService};
use opencode_mem_storage::{StorageBackend, traits::ObservationStore};
use serde_json::json;
use std::sync::Arc;

async fn setup_storage() -> StorageBackend {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
    StorageBackend::new(&url)
        .await
        .expect("Failed to connect to PG")
}

fn setup_search_service(backend: StorageBackend) -> SearchService {
    SearchService::new(Arc::new(backend), None, None)
}

fn setup_observation_service(backend: StorageBackend) -> opencode_mem_service::ObservationService {
    let (event_tx, _rx) = tokio::sync::broadcast::channel(16);
    let config = opencode_mem_core::AppConfig {
        database_url: String::new(),
        api_key: String::new(),
        api_url: String::new(),
        model: String::new(),
        disable_embeddings: true,
        embedding_threads: 0,
        infinite_memory_url: None,
        dedup_threshold: 0.85,
        injection_dedup_threshold: 0.80,
        queue_workers: 10,
        max_retry: 3,
        visibility_timeout_secs: 300,
        dlq_ttl_days: 7,
        max_content_chars: 500,
        max_total_chars: 8000,
        max_events: 200,
    };
    opencode_mem_service::ObservationService::new(
        Arc::new(backend),
        Arc::new(
            opencode_mem_llm::LlmClient::new(String::new(), String::new(), String::new()).unwrap(),
        ),
        None,
        event_tx,
        None,
        &config,
    )
}

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

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn test_memory_get_empty_id() {
    let backend = setup_storage().await;
    let search_svc = setup_search_service(backend);
    let args = json!({"id": ""});
    let result = handle_memory_get(&search_svc, &args).await;
    assert_eq!(result["isError"].as_bool(), Some(true));
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .contains("required")
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn test_memory_get_missing_id() {
    let backend = setup_storage().await;
    let search_svc = setup_search_service(backend);
    let args = json!({});
    let result = handle_memory_get(&search_svc, &args).await;
    assert_eq!(result["isError"].as_bool(), Some(true));
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .contains("required")
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn test_hybrid_search_empty_query() {
    let backend = setup_storage().await;
    let search_svc = setup_search_service(backend);
    let args = json!({"query": ""});
    let result = handle_hybrid_search(&search_svc, &args, 20).await;
    assert_eq!(result["isError"].as_bool(), Some(true));
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .contains("required")
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn test_hybrid_search_missing_query() {
    let backend = setup_storage().await;
    let search_svc = setup_search_service(backend);
    let args = json!({});
    let result = handle_hybrid_search(&search_svc, &args, 20).await;
    assert_eq!(result["isError"].as_bool(), Some(true));
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .contains("required")
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn test_semantic_search_empty_query() {
    let backend = setup_storage().await;
    let search_svc = setup_search_service(backend);
    let args = json!({"query": ""});
    let result = handle_semantic_search(&search_svc, &args, 20).await;
    assert_eq!(result["isError"].as_bool(), Some(true));
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .contains("required")
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn test_get_observations_too_many_ids() {
    let backend = setup_storage().await;
    let search_svc = setup_search_service(backend);
    let ids: Vec<String> = (0..501).map(|i| format!("id-{i}")).collect();
    let args = json!({"ids": ids});
    let result = handle_get_observations(&search_svc, &args).await;
    assert_eq!(result["isError"].as_bool(), Some(true));
    assert!(
        result["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .contains("500")
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn test_search_limit_capped() {
    let backend = setup_storage().await;
    let search_svc = setup_search_service(backend);
    let args = json!({"query": "test", "limit": 5000});
    let result = handle_search(&search_svc, &args, 1000).await;
    assert!(result.get("isError").is_none());
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
#[expect(clippy::unwrap_used, reason = "test code")]
async fn test_search_with_date_filters() {
    let backend = setup_storage().await;

    let obs = Observation::builder(
        "obs-date-1".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "date filter test observation".to_owned(),
    )
    .build();
    assert!(backend.save_observation(&obs).await.unwrap());

    let search_svc = setup_search_service(backend);

    let result = handle_search(
        &search_svc,
        &json!({"query": "date filter test", "from": "2020-01-01"}),
        50,
    )
    .await;
    assert!(result.get("isError").is_none());
    let content_text = result["content"][0]["text"].as_str().unwrap();
    let results: Vec<serde_json::Value> = serde_json::from_str(content_text).unwrap();
    assert_eq!(results.len(), 1);

    let result = handle_search(
        &search_svc,
        &json!({"query": "date filter test", "to": "2020-01-01"}),
        50,
    )
    .await;
    assert!(result.get("isError").is_none());
    let content_text = result["content"][0]["text"].as_str().unwrap();
    let results: Vec<serde_json::Value> = serde_json::from_str(content_text).unwrap();
    assert!(results.is_empty());
}
