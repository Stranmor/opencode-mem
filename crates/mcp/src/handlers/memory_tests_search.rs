use super::*;

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
