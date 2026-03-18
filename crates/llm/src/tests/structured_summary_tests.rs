use crate::ai_types::StructuredSummaryJson;

#[test]
fn parses_full_structured_summary() {
    let json = r#"{
        "summary": "Implemented structured session summaries.",
        "request": "Add structured output to session summaries",
        "investigated": "LLM prompt format and SessionSummary DB schema",
        "completed": "Updated prompt to return JSON with typed fields",
        "next_steps": "Add migration for decisions column",
        "files_read": ["crates/llm/src/summary.rs", "crates/core/src/session.rs"],
        "files_modified": ["crates/llm/src/ai_types.rs"],
        "decisions": ["Use response_format json_object for structured output"],
        "discoveries": ["SessionSummary already has structured DB columns"]
    }"#;

    let parsed: StructuredSummaryJson = serde_json::from_str(json).expect("parse full");
    assert_eq!(parsed.summary, "Implemented structured session summaries.");
    assert_eq!(
        parsed.request.as_deref(),
        Some("Add structured output to session summaries")
    );
    assert_eq!(parsed.files_read.len(), 2);
    assert_eq!(parsed.files_modified.len(), 1);
    assert_eq!(parsed.decisions.len(), 1);
    assert_eq!(parsed.discoveries.len(), 1);
}

#[test]
fn parses_minimal_summary_with_defaults() {
    let json = r#"{"summary": "Quick fix applied."}"#;

    let parsed: StructuredSummaryJson = serde_json::from_str(json).expect("parse minimal");
    assert_eq!(parsed.summary, "Quick fix applied.");
    assert!(parsed.request.is_none());
    assert!(parsed.investigated.is_none());
    assert!(parsed.completed.is_none());
    assert!(parsed.next_steps.is_none());
    assert!(parsed.files_read.is_empty());
    assert!(parsed.files_modified.is_empty());
    assert!(parsed.decisions.is_empty());
    assert!(parsed.discoveries.is_empty());
}

#[test]
fn handles_null_fields_gracefully() {
    let json = r#"{
        "summary": "Session summary.",
        "request": null,
        "investigated": null,
        "completed": null,
        "files_read": null,
        "files_modified": null,
        "decisions": null,
        "discoveries": null
    }"#;

    let parsed: StructuredSummaryJson = serde_json::from_str(json).expect("parse nulls");
    assert_eq!(parsed.summary, "Session summary.");
    assert!(parsed.request.is_none());
    assert!(parsed.files_read.is_empty());
    assert!(parsed.decisions.is_empty());
}

#[test]
fn handles_null_summary_as_empty_string() {
    let json = r#"{"summary": null}"#;

    let parsed: StructuredSummaryJson = serde_json::from_str(json).expect("null summary");
    assert!(parsed.summary.is_empty());
}

#[test]
fn filters_non_string_entries_in_arrays() {
    let json = r#"{
        "summary": "Test.",
        "files_read": ["valid.rs", 42, null, "also_valid.rs"],
        "decisions": [true, "real decision"]
    }"#;

    let parsed: StructuredSummaryJson = serde_json::from_str(json).expect("mixed arrays");
    assert_eq!(parsed.files_read, vec!["valid.rs", "also_valid.rs"]);
    assert_eq!(parsed.decisions, vec!["real decision"]);
}
