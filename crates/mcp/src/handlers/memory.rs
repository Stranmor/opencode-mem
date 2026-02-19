use opencode_mem_core::MAX_BATCH_IDS;
use opencode_mem_service::SearchService;

use super::{mcp_err, mcp_ok, mcp_text, parse_limit};

pub(super) async fn handle_search(
    search_service: &SearchService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let query = args.get("query").and_then(|q| q.as_str()).filter(|s| !s.is_empty());
    let limit = parse_limit(args);
    let project = args.get("project").and_then(|p| p.as_str());
    let obs_type = args.get("type").and_then(|t| t.as_str());
    let from = args.get("from").and_then(|f| f.as_str());
    let to = args.get("to").and_then(|t| t.as_str());

    // Use semantic search when no filters are active and query is present
    if project.is_none() && obs_type.is_none() && from.is_none() && to.is_none() {
        if let Some(q) = query {
            return match search_service.semantic_search_with_fallback(q, limit).await {
                Ok(results) => mcp_ok(&results),
                Err(e) => mcp_err(e),
            };
        }
    }

    match search_service.search_with_filters(query, project, obs_type, from, to, limit).await {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) async fn handle_timeline(
    search_service: &SearchService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let from = args.get("from").and_then(|f| f.as_str());
    let to = args.get("to").and_then(|t| t.as_str());
    let limit = parse_limit(args);
    match search_service.get_timeline(from, to, limit).await {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) async fn handle_get_observations(
    search_service: &SearchService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let ids: Vec<String> = args
        .get("ids")
        .and_then(|i| i.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(ToOwned::to_owned)).collect())
        .unwrap_or_default();
    if ids.is_empty() {
        mcp_err("ids array is required and must not be empty")
    } else if ids.len() > MAX_BATCH_IDS {
        mcp_err(format!("ids array exceeds maximum of {MAX_BATCH_IDS} items"))
    } else {
        match search_service.get_observations_by_ids(&ids).await {
            Ok(results) => mcp_ok(&results),
            Err(e) => mcp_err(e),
        }
    }
}

pub(super) async fn handle_memory_get(
    search_service: &SearchService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let Some(id_str) = args.get("id").and_then(|i| i.as_str()).filter(|s| !s.is_empty()) else {
        return mcp_err("'id' parameter is required and must not be empty");
    };
    match search_service.get_observation_by_id(id_str).await {
        Ok(Some(obs)) => mcp_ok(&obs),
        Ok(None) => mcp_text(&format!("Observation not found: {id_str}")),
        Err(e) => mcp_err(e),
    }
}

pub(super) async fn handle_memory_recent(
    search_service: &SearchService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let limit = parse_limit(args);
    match search_service.get_recent_observations(limit).await {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) async fn handle_hybrid_search(
    search_service: &SearchService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let Some(query) = args.get("query").and_then(|q| q.as_str()).filter(|s| !s.is_empty()) else {
        return mcp_err("'query' parameter is required and must not be empty");
    };
    let limit = parse_limit(args);
    match search_service.hybrid_search(query, limit).await {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) async fn handle_semantic_search(
    search_service: &SearchService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let Some(query) = args.get("query").and_then(|q| q.as_str()).filter(|s| !s.is_empty()) else {
        return mcp_err("'query' parameter is required and must not be empty");
    };
    let limit = parse_limit(args);

    match search_service.semantic_search_with_fallback(query, limit).await {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) async fn handle_save_memory(
    observation_service: &opencode_mem_service::ObservationService,
    args: &serde_json::Value,
) -> serde_json::Value {
    let raw_text = match args.get("text").and_then(|t| t.as_str()) {
        Some(text) => text.trim(),
        None => return mcp_err("text is required and must be a string"),
    };
    if raw_text.is_empty() {
        return mcp_err("text is required and must not be empty");
    }

    let title = args.get("title").and_then(|t| t.as_str());
    let project = args.get("project").and_then(|p| p.as_str());

    match observation_service.save_memory(raw_text, title, project).await {
        Ok(Some(obs)) => mcp_ok(&obs),
        Ok(None) => mcp_text("Observation filtered as low-value"),
        Err(e) => mcp_err(e),
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "test code")]
#[expect(clippy::indexing_slicing, reason = "test code — asserts guard length")]
mod tests {
    use super::*;
    use opencode_mem_core::{Observation, ObservationType};
    use opencode_mem_storage::{StorageBackend, traits::ObservationStore};
    use serde_json::json;
    use std::sync::Arc;

    async fn setup_storage() -> StorageBackend {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
        StorageBackend::new(&url).await.expect("Failed to connect to PG")
    }

    fn setup_search_service(backend: StorageBackend) -> SearchService {
        SearchService::new(Arc::new(backend), None)
    }

    fn setup_observation_service(
        backend: StorageBackend,
    ) -> opencode_mem_service::ObservationService {
        let (event_tx, _rx) = tokio::sync::broadcast::channel(16);
        opencode_mem_service::ObservationService::new(
            Arc::new(backend),
            Arc::new(opencode_mem_llm::LlmClient::new(String::new(), String::new()).unwrap()),
            None,
            event_tx,
            None,
        )
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_save_memory_missing_text() {
        let backend = setup_storage().await;
        let obs_service = setup_observation_service(backend);
        let args = json!({});
        let result = handle_save_memory(&obs_service, &args).await;

        assert_eq!(result["isError"].as_bool(), Some(true));
        assert!(result["content"][0]["text"].as_str().unwrap().contains("text is required"));
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_save_memory_empty_text() {
        let backend = setup_storage().await;
        let obs_service = setup_observation_service(backend);
        let args = json!({ "text": "  " });
        let result = handle_save_memory(&obs_service, &args).await;

        assert_eq!(result["isError"].as_bool(), Some(true));
        assert!(result["content"][0]["text"].as_str().unwrap().contains("must not be empty"));
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_save_memory_with_title() {
        let backend = setup_storage().await;
        let obs_service = setup_observation_service(backend);
        let args = json!({
            "text": "some narrative",
            "title": "custom title"
        });
        let result = handle_save_memory(&obs_service, &args).await;

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
        let long_text = "A very long text that should be truncated for the title because it is more than fifty characters long.";
        let args = json!({
            "text": long_text
        });
        let result = handle_save_memory(&obs_service, &args).await;

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
        let args = json!({
            "text": "narrative",
            "project": "test-project"
        });
        let result = handle_save_memory(&obs_service, &args).await;

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
        let args = json!({
            "text": "success test"
        });
        let result = handle_save_memory(&obs_service, &args).await;

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
        assert!(result["content"][0]["text"].as_str().unwrap_or("").contains("required"));
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_memory_get_missing_id() {
        let backend = setup_storage().await;
        let search_svc = setup_search_service(backend);
        let args = json!({});
        let result = handle_memory_get(&search_svc, &args).await;
        assert_eq!(result["isError"].as_bool(), Some(true));
        assert!(result["content"][0]["text"].as_str().unwrap_or("").contains("required"));
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_hybrid_search_empty_query() {
        let backend = setup_storage().await;
        let search_svc = setup_search_service(backend);
        let args = json!({"query": ""});
        let result = handle_hybrid_search(&search_svc, &args).await;
        assert_eq!(result["isError"].as_bool(), Some(true));
        assert!(result["content"][0]["text"].as_str().unwrap_or("").contains("required"));
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_hybrid_search_missing_query() {
        let backend = setup_storage().await;
        let search_svc = setup_search_service(backend);
        let args = json!({});
        let result = handle_hybrid_search(&search_svc, &args).await;
        assert_eq!(result["isError"].as_bool(), Some(true));
        assert!(result["content"][0]["text"].as_str().unwrap_or("").contains("required"));
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_semantic_search_empty_query() {
        let backend = setup_storage().await;
        let search_svc = setup_search_service(backend);
        let args = json!({"query": ""});
        let result = handle_semantic_search(&search_svc, &args).await;
        assert_eq!(result["isError"].as_bool(), Some(true));
        assert!(result["content"][0]["text"].as_str().unwrap_or("").contains("required"));
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
        assert!(result["content"][0]["text"].as_str().unwrap_or("").contains("500"));
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn test_search_limit_capped() {
        let backend = setup_storage().await;
        let search_svc = setup_search_service(backend);
        let args = json!({"query": "test", "limit": 5000});
        let result = handle_search(&search_svc, &args).await;
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

        let result =
            handle_search(&search_svc, &json!({"query": "date filter test", "from": "2020-01-01"}))
                .await;
        assert!(result.get("isError").is_none());
        let content_text = result["content"][0]["text"].as_str().unwrap();
        let results: Vec<serde_json::Value> = serde_json::from_str(content_text).unwrap();
        assert_eq!(results.len(), 1);

        let result =
            handle_search(&search_svc, &json!({"query": "date filter test", "to": "2020-01-01"}))
                .await;
        assert!(result.get("isError").is_none());
        let content_text = result["content"][0]["text"].as_str().unwrap();
        let results: Vec<serde_json::Value> = serde_json::from_str(content_text).unwrap();
        assert!(results.is_empty());
    }
}
