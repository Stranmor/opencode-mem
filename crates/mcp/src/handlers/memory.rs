use opencode_mem_core::{is_low_value_observation, Observation, ObservationType};
use opencode_mem_embeddings::EmbeddingProvider;
use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_storage::Storage;
use std::sync::Arc;
use tokio::runtime::Handle;

use super::{mcp_err, mcp_ok, mcp_text};

pub(super) fn handle_search(storage: &Storage, args: &serde_json::Value) -> serde_json::Value {
    let query = args.get("query").and_then(|q| q.as_str());
    let limit = args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(50) as usize;
    let project = args.get("project").and_then(|p| p.as_str());
    let obs_type = args.get("type").and_then(|t| t.as_str());
    match storage.search_with_filters(query, project, obs_type, limit) {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_timeline(storage: &Storage, args: &serde_json::Value) -> serde_json::Value {
    let from = args.get("from").and_then(|f| f.as_str());
    let to = args.get("to").and_then(|t| t.as_str());
    let limit = args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(50) as usize;
    match storage.get_timeline(from, to, limit) {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_get_observations(
    storage: &Storage,
    args: &serde_json::Value,
) -> serde_json::Value {
    let ids: Vec<String> = args
        .get("ids")
        .and_then(|i| i.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(ToOwned::to_owned)).collect())
        .unwrap_or_default();
    if ids.is_empty() {
        mcp_err("ids array is required and must not be empty")
    } else {
        match storage.get_observations_by_ids(&ids) {
            Ok(results) => mcp_ok(&results),
            Err(e) => mcp_err(e),
        }
    }
}

pub(super) fn handle_memory_get(storage: &Storage, args: &serde_json::Value) -> serde_json::Value {
    let id_str = args.get("id").and_then(|i| i.as_str()).unwrap_or("");
    match storage.get_by_id(id_str) {
        Ok(Some(obs)) => mcp_ok(&obs),
        Ok(None) => mcp_text(&format!("Observation not found: {id_str}")),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_memory_recent(
    storage: &Storage,
    args: &serde_json::Value,
) -> serde_json::Value {
    let limit = args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(10) as usize;
    match storage.get_recent(limit) {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_hybrid_search(
    storage: &Storage,
    args: &serde_json::Value,
) -> serde_json::Value {
    let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
    let limit = args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(20) as usize;
    match storage.hybrid_search(query, limit) {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_semantic_search(
    storage: &Storage,
    embeddings: Option<&EmbeddingService>,
    args: &serde_json::Value,
) -> serde_json::Value {
    let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
    let limit = args.get("limit").and_then(serde_json::Value::as_u64).unwrap_or(20) as usize;

    match opencode_mem_search::run_semantic_search_with_fallback(storage, embeddings, query, limit)
    {
        Ok(results) => mcp_ok(&results),
        Err(e) => mcp_err(e),
    }
}

pub(super) fn handle_save_memory(
    storage: &Storage,
    embeddings: Option<Arc<EmbeddingService>>,
    _handle: &Handle,
    args: &serde_json::Value,
) -> serde_json::Value {
    let raw_text = match args.get("text").and_then(|t| t.as_str()) {
        Some(text) => text.trim(),
        None => return mcp_err("text is required and must be a string"),
    };
    if raw_text.is_empty() {
        return mcp_err("text is required and must not be empty");
    }

    let title = args
        .get("title")
        .and_then(|t| t.as_str())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| raw_text.chars().take(50).collect());
    let project = args
        .get("project")
        .and_then(|p| p.as_str())
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(ToOwned::to_owned);

    let observation = Observation::builder(
        uuid::Uuid::new_v4().to_string(),
        "manual".to_owned(),
        ObservationType::Discovery,
        title,
    )
    .maybe_project(project)
    .narrative(raw_text.to_owned())
    .build();

    if is_low_value_observation(&observation.title) {
        tracing::debug!("Filtered low-value MCP save_memory: {}", observation.title);
        return mcp_text("Observation filtered as low-value");
    }

    match storage.save_observation(&observation) {
        Ok(true) => {},
        Ok(false) => {
            tracing::debug!("Duplicate MCP save_memory: {}", observation.title);
            return mcp_text("Duplicate observation (same title already exists)");
        },
        Err(e) => return mcp_err(e),
    }

    if let Some(emb) = embeddings {
        let storage = storage.clone();
        let obs_id = observation.id.clone();
        let embedding_text = format!(
            "{} {} {}",
            observation.title,
            observation.narrative.as_deref().unwrap_or(""),
            observation.facts.join(" ")
        );

        _handle.spawn(async move {
            let embedding_result =
                tokio::task::spawn_blocking(move || emb.embed(&embedding_text)).await;

            match embedding_result {
                Ok(Ok(vec)) => {
                    if let Err(e) = storage.store_embedding(&obs_id, &vec) {
                        tracing::warn!("Failed to store embedding for {}: {}", obs_id, e);
                    }
                },
                Ok(Err(e)) => {
                    tracing::warn!("Failed to generate embedding for {}: {}", obs_id, e);
                },
                Err(e) => {
                    tracing::warn!("Embedding task join error for {}: {}", obs_id, e);
                },
            }
        });
    }

    mcp_ok(&observation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn setup_storage() -> (Storage, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let storage = Storage::new(&db_path).unwrap();
        (storage, dir)
    }

    #[tokio::test]
    async fn test_save_memory_missing_text() {
        let (storage, _dir) = setup_storage();
        let handle = tokio::runtime::Handle::current();
        let args = json!({});
        let result = handle_save_memory(&storage, None, &handle, &args);

        assert_eq!(result["isError"].as_bool(), Some(true));
        assert!(result["content"][0]["text"].as_str().unwrap().contains("text is required"));
    }

    #[tokio::test]
    async fn test_save_memory_empty_text() {
        let (storage, _dir) = setup_storage();
        let handle = tokio::runtime::Handle::current();
        let args = json!({ "text": "  " });
        let result = handle_save_memory(&storage, None, &handle, &args);

        assert_eq!(result["isError"].as_bool(), Some(true));
        assert!(result["content"][0]["text"].as_str().unwrap().contains("must not be empty"));
    }

    #[tokio::test]
    async fn test_save_memory_with_title() {
        let (storage, _dir) = setup_storage();
        let handle = tokio::runtime::Handle::current();
        let args = json!({
            "text": "some narrative",
            "title": "custom title"
        });
        let result = handle_save_memory(&storage, None, &handle, &args);

        assert!(result.get("isError").is_none());
        let obs_json = result["content"][0]["text"].as_str().unwrap();
        let obs: Observation = serde_json::from_str(obs_json).unwrap();
        assert_eq!(obs.title, "custom title");
        assert_eq!(obs.narrative.as_deref(), Some("some narrative"));

        let saved = storage.get_by_id(&obs.id).unwrap().unwrap();
        assert_eq!(saved.title, "custom title");
    }

    #[tokio::test]
    async fn test_save_memory_without_title() {
        let (storage, _dir) = setup_storage();
        let handle = tokio::runtime::Handle::current();
        let long_text = "A very long text that should be truncated for the title because it is more than fifty characters long.";
        let args = json!({
            "text": long_text
        });
        let result = handle_save_memory(&storage, None, &handle, &args);

        assert!(result.get("isError").is_none());
        let obs_json = result["content"][0]["text"].as_str().unwrap();
        let obs: Observation = serde_json::from_str(obs_json).unwrap();
        assert_eq!(obs.title.chars().count(), 50);
        assert!(long_text.starts_with(&obs.title));
    }

    #[tokio::test]
    async fn test_save_memory_with_project() {
        let (storage, _dir) = setup_storage();
        let handle = tokio::runtime::Handle::current();
        let args = json!({
            "text": "narrative",
            "project": "test-project"
        });
        let result = handle_save_memory(&storage, None, &handle, &args);

        assert!(result.get("isError").is_none());
        let obs_json = result["content"][0]["text"].as_str().unwrap();
        let obs: Observation = serde_json::from_str(obs_json).unwrap();
        assert_eq!(obs.project.as_deref(), Some("test-project"));
    }

    #[tokio::test]
    async fn test_save_memory_success_returns_observation() {
        let (storage, _dir) = setup_storage();
        let handle = tokio::runtime::Handle::current();
        let args = json!({
            "text": "success test"
        });
        let result = handle_save_memory(&storage, None, &handle, &args);

        assert!(result.get("isError").is_none());
        let content = &result["content"][0];
        assert_eq!(content["type"], "text");
        let obs_json = content["text"].as_str().unwrap();
        let _: Observation =
            serde_json::from_str(obs_json).expect("Should return valid Observation JSON");
    }
}
