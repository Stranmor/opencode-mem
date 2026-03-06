use super::*;

#[tokio::test]
async fn all_tools_degrade_gracefully_without_database() {
    let (observation_service, session_service, knowledge_service, search_service, pending_writes) =
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
            &pending_writes,
            &handle,
            &params,
            json!(1),
        )
        .await;

        let result = response.result.as_ref().unwrap_or_else(|| {
            panic!(
                "Tool '{tool_name}' returned no result (has error: {:?})",
                response.error
            )
        });

        let is_error = result
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

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
        assert!(
            !text.is_empty(),
            "Tool '{tool_name}' returned empty text in degraded mode"
        );
    }
}

#[tokio::test]
async fn read_tools_return_empty_results_in_degraded_mode() {
    let (observation_service, session_service, knowledge_service, search_service, pending_writes) =
        setup_degraded_services();
    let handle = tokio::runtime::Handle::current();

    // Tools that return empty JSON arrays when degraded
    let array_tools = [
        "search",
        "timeline",
        "get_observations",
        "memory_recent",
        "memory_hybrid_search",
        "memory_semantic_search",
        "knowledge_search",
        "knowledge_list",
    ];

    for tool_name in array_tools {
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
            &pending_writes,
            &handle,
            &params,
            json!(1),
        )
        .await;

        let result = response.result.as_ref().unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
        let arr = parsed.as_array().unwrap_or_else(|| {
            panic!("Read tool '{tool_name}' should return a JSON array, got: {text}")
        });
        assert!(
            arr.is_empty(),
            "Read tool '{tool_name}' should return an empty array in degraded mode, got {len} items",
            len = arr.len()
        );
    }

    let single_lookup_tools = ["memory_get", "knowledge_get"];

    for tool_name in single_lookup_tools {
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
            &pending_writes,
            &handle,
            &params,
            json!(1),
        )
        .await;

        let result = response.result.as_ref().unwrap();
        let is_error = result
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        assert!(
            !is_error,
            "Read tool '{tool_name}' returned isError=true in degraded mode"
        );

        let text = result["content"][0]["text"].as_str().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
        let arr = parsed.as_array().unwrap_or_else(|| {
            panic!(
                "Read tool '{tool_name}' should return a JSON array in degraded mode, got: {text}"
            )
        });
        assert!(
            arr.is_empty(),
            "Read tool '{tool_name}' should return an empty array in degraded mode, got {len} items",
            len = arr.len()
        );
    }
}
