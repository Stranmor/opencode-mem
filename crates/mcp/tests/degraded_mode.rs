#![allow(clippy::unwrap_used, reason = "test code")]
#![allow(clippy::indexing_slicing, reason = "test code with known structure")]
#![allow(clippy::missing_docs_in_private_items, reason = "test code")]
#![allow(missing_docs, reason = "test code")]
#![allow(clippy::implicit_return, reason = "test code")]
#![allow(clippy::question_mark_used, reason = "test code")]

use opencode_mem_mcp::McpTool;
use opencode_mem_service::{KnowledgeService, ObservationService, SearchService, SessionService};
use opencode_mem_storage::StorageBackend;
use serde_json::json;
use std::sync::Arc;

fn setup_degraded_services()
-> (Arc<ObservationService>, Arc<SessionService>, Arc<KnowledgeService>, Arc<SearchService>) {
    let backend =
        Arc::new(StorageBackend::new_degraded("postgres://bogus:bogus@127.0.0.1:1/bogus"));

    let (event_tx, _rx) = tokio::sync::broadcast::channel(16);
    let llm = Arc::new(opencode_mem_llm::LlmClient::new(String::new(), String::new()).unwrap());

    let observation_service =
        Arc::new(ObservationService::new(backend.clone(), llm.clone(), None, event_tx, None));
    let session_service = Arc::new(SessionService::new(backend.clone(), llm));
    let knowledge_service = Arc::new(KnowledgeService::new(backend.clone()));
    let search_service = Arc::new(SearchService::new(backend, None));

    (observation_service, session_service, knowledge_service, search_service)
}

fn tool_args(tool_name: &str) -> serde_json::Value {
    match tool_name {
        "__IMPORTANT" => json!({}),
        "search" => json!({"query": "test"}),
        "timeline" => json!({}),
        "get_observations" => json!({"ids": ["test-id-1"]}),
        "memory_get" => json!({"id": "nonexistent-id"}),
        "memory_recent" => json!({}),
        "memory_hybrid_search" => json!({"query": "test"}),
        "memory_semantic_search" => json!({"query": "test"}),
        "save_memory" => json!({"text": "test memory content"}),
        "knowledge_search" => json!({"query": "test"}),
        "knowledge_save" => json!({
            "knowledge_type": "skill",
            "title": "Test Skill",
            "description": "A test skill description"
        }),
        "knowledge_get" => json!({"id": "nonexistent-id"}),
        "knowledge_list" => json!({}),
        "knowledge_delete" => json!({"id": "nonexistent-id"}),
        "infinite_expand" => json!({"id": 1}),
        "infinite_time_range" => json!({
            "start": "2025-01-01T00:00:00Z",
            "end": "2025-01-02T00:00:00Z"
        }),
        "infinite_drill_hour" => json!({"id": 1}),
        "infinite_drill_minute" => json!({"id": 1}),
        _ => json!({}),
    }
}

const INFINITE_TOOLS: [&str; 4] =
    ["infinite_expand", "infinite_time_range", "infinite_drill_hour", "infinite_drill_minute"];

#[tokio::test]
async fn all_tools_degrade_gracefully_without_database() {
    let (observation_service, session_service, knowledge_service, search_service) =
        setup_degraded_services();
    let handle = tokio::runtime::Handle::current();

    for tool_name in McpTool::all_tool_names() {
        let args = tool_args(tool_name);
        let params = json!({
            "name": tool_name,
            "arguments": args,
        });

        let response = opencode_mem_mcp::handle_tool_call(
            None,
            &observation_service,
            &session_service,
            &knowledge_service,
            &search_service,
            &handle,
            &params,
            json!(1),
        )
        .await;

        let result = response.result.as_ref().unwrap_or_else(|| {
            panic!("Tool '{tool_name}' returned no result (has error: {:?})", response.error)
        });

        let is_error = result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);

        // Infinite tools return isError when not configured (INFINITE_MEMORY_URL not set)
        // — this is a config error, not a degradation failure
        if INFINITE_TOOLS.contains(&tool_name) {
            assert!(
                is_error,
                "Infinite tool '{tool_name}' should return config error when not configured"
            );
            continue;
        }

        assert!(
            !is_error,
            "Tool '{tool_name}' returned isError=true in degraded mode. Response: {}",
            serde_json::to_string_pretty(result).unwrap()
        );

        let content = result
            .get("content")
            .and_then(|c| c.as_array())
            .expect("response should have content array");
        assert!(
            !content.is_empty(),
            "Tool '{tool_name}' returned empty content array in degraded mode"
        );

        let text = content[0]
            .get("text")
            .and_then(|t| t.as_str())
            .expect("content[0] should have text field");
        assert!(!text.is_empty(), "Tool '{tool_name}' returned empty text in degraded mode");
    }
}

#[tokio::test]
async fn read_tools_return_empty_results_in_degraded_mode() {
    let (_observation_service, _session_service, _knowledge_service, _search_service) =
        setup_degraded_services();
    let handle = tokio::runtime::Handle::current();

    let read_tools = [
        "search",
        "timeline",
        "get_observations",
        "memory_recent",
        "memory_hybrid_search",
        "memory_semantic_search",
        "knowledge_search",
        "knowledge_list",
    ];

    for tool_name in read_tools {
        let args = tool_args(tool_name);
        let params = json!({
            "name": tool_name,
            "arguments": args,
        });

        let response = opencode_mem_mcp::handle_tool_call(
            None,
            &Arc::new(ObservationService::new(
                Arc::new(StorageBackend::new_degraded("postgres://bogus:bogus@127.0.0.1:1/bogus")),
                Arc::new(opencode_mem_llm::LlmClient::new(String::new(), String::new()).unwrap()),
                None,
                tokio::sync::broadcast::channel(16).0,
                None,
            )),
            &Arc::new(SessionService::new(
                Arc::new(StorageBackend::new_degraded("postgres://bogus:bogus@127.0.0.1:1/bogus")),
                Arc::new(opencode_mem_llm::LlmClient::new(String::new(), String::new()).unwrap()),
            )),
            &Arc::new(KnowledgeService::new(Arc::new(StorageBackend::new_degraded(
                "postgres://bogus:bogus@127.0.0.1:1/bogus",
            )))),
            &Arc::new(SearchService::new(
                Arc::new(StorageBackend::new_degraded("postgres://bogus:bogus@127.0.0.1:1/bogus")),
                None,
            )),
            &handle,
            &params,
            json!(1),
        )
        .await;

        let result = response.result.as_ref().unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(
            parsed.as_array().is_some(),
            "Read tool '{tool_name}' should return a JSON array, got: {text}"
        );
    }
}

#[tokio::test]
async fn write_tools_return_degraded_message() {
    let (observation_service, session_service, knowledge_service, search_service) =
        setup_degraded_services();
    let handle = tokio::runtime::Handle::current();

    let write_tools = ["save_memory", "knowledge_save", "knowledge_delete"];

    for tool_name in write_tools {
        let args = tool_args(tool_name);
        let params = json!({
            "name": tool_name,
            "arguments": args,
        });

        let response = opencode_mem_mcp::handle_tool_call(
            None,
            &observation_service,
            &session_service,
            &knowledge_service,
            &search_service,
            &handle,
            &params,
            json!(1),
        )
        .await;

        let result = response.result.as_ref().unwrap();
        let is_error = result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
        assert!(!is_error, "Write tool '{tool_name}' returned isError=true in degraded mode");

        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("degraded") || text.contains("unavailable") || text.contains("skipped"),
            "Write tool '{tool_name}' should indicate degraded mode in its response, got: {text}"
        );
    }
}
