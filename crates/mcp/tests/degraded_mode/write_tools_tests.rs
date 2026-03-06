use super::*;

#[tokio::test]
async fn write_tools_return_degraded_message() {
    let (observation_service, session_service, knowledge_service, search_service, pending_writes) =
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
            "Write tool '{tool_name}' returned isError=true in degraded mode"
        );

        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("degraded")
                || text.contains("unavailable")
                || text.contains("skipped")
                || text.contains("buffered"),
            "Write tool '{tool_name}' should indicate degraded mode in its response, got: {text}"
        );
    }
}
