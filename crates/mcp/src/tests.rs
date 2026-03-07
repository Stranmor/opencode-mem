use super::*;
use handlers::{mcp_err, mcp_ok, mcp_text};

#[test]
fn test_mcp_tool_parse_valid() {
    assert_eq!(McpTool::parse("search"), Some(McpTool::Search));
    assert_eq!(McpTool::parse("timeline"), Some(McpTool::Timeline));
    assert_eq!(
        McpTool::parse("get_observations"),
        Some(McpTool::GetObservations)
    );
    assert_eq!(McpTool::parse("memory_get"), Some(McpTool::MemoryGet));
    assert_eq!(McpTool::parse("memory_recent"), Some(McpTool::MemoryRecent));
    assert_eq!(
        McpTool::parse("memory_hybrid_search"),
        Some(McpTool::MemoryHybridSearch)
    );
    assert_eq!(
        McpTool::parse("memory_semantic_search"),
        Some(McpTool::MemorySemanticSearch)
    );
    assert_eq!(McpTool::parse("save_memory"), Some(McpTool::SaveMemory));
    assert_eq!(McpTool::parse("__IMPORTANT"), Some(McpTool::Important));
    assert_eq!(
        McpTool::parse("knowledge_search"),
        Some(McpTool::KnowledgeSearch)
    );
    assert_eq!(
        McpTool::parse("knowledge_save"),
        Some(McpTool::KnowledgeSave)
    );
    assert_eq!(McpTool::parse("knowledge_get"), Some(McpTool::KnowledgeGet));
    assert_eq!(
        McpTool::parse("knowledge_list"),
        Some(McpTool::KnowledgeList)
    );
    assert_eq!(
        McpTool::parse("knowledge_delete"),
        Some(McpTool::KnowledgeDelete)
    );
    assert_eq!(
        McpTool::parse("infinite_expand"),
        Some(McpTool::InfiniteExpand)
    );
    assert_eq!(
        McpTool::parse("infinite_time_range"),
        Some(McpTool::InfiniteTimeRange)
    );
    assert_eq!(
        McpTool::parse("infinite_drill_day"),
        Some(McpTool::InfiniteDrillDay)
    );
    assert_eq!(
        McpTool::parse("infinite_drill_hour"),
        Some(McpTool::InfiniteDrillHour)
    );
    assert_eq!(
        McpTool::parse("infinite_search_entities"),
        Some(McpTool::InfiniteSearchEntities)
    );
}

#[test]
fn test_mcp_tool_parse_invalid() {
    assert_eq!(McpTool::parse("unknown_tool"), None);
    assert_eq!(McpTool::parse(""), None);
    assert_eq!(McpTool::parse("SEARCH"), None);
    assert_eq!(McpTool::parse("search "), None);
}

#[test]
#[expect(clippy::indexing_slicing, reason = "test code with known structure")]
fn test_mcp_ok_serialization() {
    let data = vec!["item1", "item2"];
    let result = mcp_ok(&data);
    assert!(result.get("content").is_some());
    assert_eq!(result["content"][0]["type"].as_str(), Some("text"));
    assert!(result.get("isError").is_none());
}

#[test]
#[expect(clippy::indexing_slicing, reason = "test code with known structure")]
#[expect(clippy::unwrap_used, reason = "test code")]
fn test_mcp_err_format() {
    let result = mcp_err("test error");
    assert_eq!(result["isError"].as_bool(), Some(true));
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Error: test error"));
}

#[test]
#[expect(clippy::indexing_slicing, reason = "test code with known structure")]
fn test_mcp_text_format() {
    let result = mcp_text("hello world");
    assert_eq!(result["content"][0]["text"].as_str(), Some("hello world"));
    assert!(result.get("isError").is_none());
}

/// Regression: no SQLite-specific terminology in tool descriptions.
/// Vulnerability IDs: 221, 222
#[test]
fn test_no_sqlite_references_in_tool_descriptions() {
    let tools_json = get_tools_json();
    let tools_str =
        serde_json::to_string(&tools_json).expect("tools JSON serialization should not fail");
    let tools_lower = tools_str.to_lowercase();

    let banned_terms = ["fts5", "sqlite", "sqlite-vec", "vec0", "load_extension"];

    for term in banned_terms {
        assert!(
            !tools_lower.contains(term),
            "MCP tool descriptions contain banned SQLite term '{term}'. \
             SQLite backend was removed — use PostgreSQL terminology (tsvector, pgvector). \
             Found in: {tools_str}"
        );
    }
}
