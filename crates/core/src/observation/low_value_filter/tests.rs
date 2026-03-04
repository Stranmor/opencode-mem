use super::{LowValueFilter, is_low_value_observation};

fn as_strs(v: &[Box<str>]) -> Vec<&str> {
    v.iter().map(|x| x.as_ref()).collect()
}

#[test]
fn test_parsing() {
    let f = LowValueFilter::from_pattern_str("a,^b,=c, ,a,^b,=c");
    assert_eq!(as_strs(&f.contains), vec!["a"]);
    assert_eq!(as_strs(&f.prefixes), vec!["b"]);
    assert_eq!(as_strs(&f.exact), vec!["c"]);
}

#[test]
fn test_filtering() {
    let low = [
        "File edit applied successfully",
        "rustfmt nightly formatting",
        "Agent behavioral protocol update",
        "Updated TODO list",
        "Search results for auth",
        "keyword frequency analysis",
        "Successful deployment to production VPS",
        "Task list update for tool result handling",
        "Routine file modification",
        "Strategy for memory deduplication",
        "WIP: Prometheus v15 Port",
        "Hybrid search API test execution",
        "Explored root directory structure",
        "Tasks marked as completed",
        "Updated warp_provisioner.rs",
        "Discovered ExportPreset and Exporter structures",
        "Syntax error in crates/hermes-ai/src/model_roles.rs",
        "Refactor plan for splitting IsolationManager",
    ];
    for title in low {
        assert!(is_low_value_observation(title), "Should be low value: {}", title);
    }
    let high = [
        "Database migration v10 added session_summaries",
        "Fixed race condition",
        "Fixing critical bug",
        "Refined scoring logic",
        "",
    ];
    for title in high {
        assert!(!is_low_value_observation(title), "Should be high value: {}", title);
    }
}

#[test]
fn test_case_and_partial() {
    assert!(is_low_value_observation("FILE EDIT APPLIED SUCCESSFULLY"));
    assert!(is_low_value_observation("There is no significant change"));
}

#[test]
fn test_unicode_bypass_prevention() {
    assert!(is_low_value_observation("Upd\u{0430}ted test.rs"));
    assert!(is_low_value_observation("Updated\u{200B}test.rs"));
}

#[test]
fn test_trivial_tool_call_evasion() {
    let malicious_cmd = serde_json::json!({"command": "ls -l; rm -rf /"});
    assert!(
        !super::is_trivial_tool_call("bash", &malicious_cmd),
        "Vulnerability exists: command chaining bypasses filter"
    );
}
