use crate::client::truncate;
use crate::observation::parse_concept;
use opencode_mem_core::{strip_markdown_json, Concept, ObservationInput, ToolOutput};

// Integration tests for observation filtering (require ANTIGRAVITY_API_KEY)
#[cfg(test)]
mod filtering_tests {
    use super::*;
    use crate::client::LlmClient;

    fn create_client() -> Option<LlmClient> {
        let api_key = std::env::var("ANTIGRAVITY_API_KEY").ok()?;
        Some(
            LlmClient::new(api_key, "https://antigravity.quantumind.ru".to_string())
                .with_model("gemini-3-flash".to_string()),
        )
    }

    fn make_input(tool: &str, title: &str, output: &str) -> ObservationInput {
        ObservationInput {
            tool: tool.to_string(),
            session_id: "test-session".to_string(),
            call_id: format!("test-call-{}", uuid::Uuid::new_v4()),
            output: ToolOutput {
                title: title.to_string(),
                output: output.to_string(),
                metadata: serde_json::Value::Null,
            },
        }
    }

    /// Test: Generic programming pattern should NOT be saved (LLM already knows)
    #[tokio::test]
    async fn test_generic_pattern_not_saved() {
        let Some(client) = create_client() else {
            eprintln!("Skipping test: ANTIGRAVITY_API_KEY not set");
            return;
        };

        let input = make_input(
            "edit",
            "Fixed race condition using RwLock",
            r#"Changed from:
    let user = cache.get(&id);
To:
    let user = cache.read().await.get(&id).cloned();
    
Standard fix for race condition - use RwLock instead of direct access."#,
        );

        let result =
            client.compress_to_observation("test-generic", &input, Some("test-project")).await;

        match result {
            Ok(None) => println!("✅ PASS: Generic pattern correctly filtered (LLM knows this)"),
            Ok(Some(obs)) => panic!("❌ FAIL: Generic pattern was saved: {}", obs.title),
            Err(e) => panic!("❌ ERROR: {}", e),
        }
    }

    /// Test: Project-specific decision SHOULD be saved
    #[tokio::test]
    async fn test_project_decision_saved() {
        let Some(client) = create_client() else {
            eprintln!("Skipping test: ANTIGRAVITY_API_KEY not set");
            return;
        };

        let input = make_input(
            "edit",
            "Architecture decision: chose sqlite-vec over pgvector",
            r#"Decision for opencode-mem project:

We chose sqlite-vec instead of pgvector because:
1. Single binary deployment - no PostgreSQL dependency
2. Embedded database - simpler ops for CLI tool
3. sqlite-vec supports cosine similarity which is enough for our use case

Trade-off: pgvector has better performance at scale, but we prioritize simplicity."#,
        );

        let result =
            client.compress_to_observation("test-decision", &input, Some("opencode-mem")).await;

        match result {
            Ok(Some(obs)) => {
                println!("✅ PASS: Project decision saved: {}", obs.title);
                assert!(obs.narrative.is_some(), "Decision should have reasoning");
            },
            Ok(None) => panic!("❌ FAIL: Project-specific decision was incorrectly filtered"),
            Err(e) => panic!("❌ ERROR: {}", e),
        }
    }

    /// Test: Project-specific gotcha SHOULD be saved
    #[tokio::test]
    async fn test_project_gotcha_saved() {
        let Some(client) = create_client() else {
            eprintln!("Skipping test: ANTIGRAVITY_API_KEY not set");
            return;
        };

        let input = make_input(
            "bash",
            "Discovered: opencode-mem binary name differs from crate name",
            r#"Error: command not found: opencode-mem-cli

Investigation: The binary is named 'opencode-mem', not 'opencode-mem-cli'.
This is because Cargo.toml defines:
  [[bin]]
  name = "opencode-mem"
  path = "src/main.rs"

Anyone new to this project would expect opencode-mem-cli based on crate name."#,
        );

        let result =
            client.compress_to_observation("test-gotcha", &input, Some("opencode-mem")).await;

        match result {
            Ok(Some(obs)) => {
                println!("✅ PASS: Project gotcha saved: {}", obs.title);
            },
            Ok(None) => panic!("❌ FAIL: Project-specific gotcha was incorrectly filtered"),
            Err(e) => panic!("❌ ERROR: {}", e),
        }
    }

    /// Test: Simple file read should NOT be saved
    #[tokio::test]
    async fn test_simple_file_read_not_saved() {
        let Some(client) = create_client() else {
            eprintln!("Skipping test: ANTIGRAVITY_API_KEY not set");
            return;
        };

        let input = make_input(
            "read",
            "Read Cargo.toml",
            r#"[package]
name = "my-project"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = "1.0""#,
        );

        let result = client.compress_to_observation("test-4", &input, Some("test-project")).await;

        match result {
            Ok(None) => println!("✅ PASS: Simple file read correctly filtered"),
            Ok(Some(obs)) => panic!("❌ FAIL: Trivial file read was saved: {}", obs.title),
            Err(e) => panic!("❌ ERROR: {}", e),
        }
    }
}

#[test]
fn test_truncate_within_limit() {
    assert_eq!(truncate("hello", 10), "hello");
}

#[test]
fn test_truncate_at_limit() {
    assert_eq!(truncate("hello", 5), "hello");
}

#[test]
fn test_truncate_exceeds_limit() {
    assert_eq!(truncate("hello world", 5), "hello");
}

#[test]
fn test_truncate_unicode_boundary() {
    let s = "привет";
    let result = truncate(s, 4);
    assert!(result.len() <= 4);
}

#[test]
fn test_truncate_empty() {
    assert_eq!(truncate("", 10), "");
}

#[test]
fn test_strip_markdown_json_clean() {
    assert_eq!(strip_markdown_json(r#"{"key": "value"}"#), r#"{"key": "value"}"#);
}

#[test]
fn test_strip_markdown_json_with_fence() {
    let input = "```json\n{\"key\": \"value\"}\n```";
    assert_eq!(strip_markdown_json(input), r#"{"key": "value"}"#);
}

#[test]
fn test_strip_markdown_json_with_generic_fence() {
    let input = "```\n{\"key\": \"value\"}\n```";
    assert_eq!(strip_markdown_json(input), r#"{"key": "value"}"#);
}

#[test]
fn test_strip_markdown_json_with_whitespace() {
    let input = "  \n```json\n{\"key\": \"value\"}\n```  \n";
    assert_eq!(strip_markdown_json(input), r#"{"key": "value"}"#);
}

#[test]
fn test_parse_concept_valid() {
    assert_eq!(parse_concept("how-it-works"), Some(Concept::HowItWorks));
    assert_eq!(parse_concept("pattern"), Some(Concept::Pattern));
    assert_eq!(parse_concept("gotcha"), Some(Concept::Gotcha));
    assert_eq!(parse_concept("trade-off"), Some(Concept::TradeOff));
}

#[test]
fn test_parse_concept_case_insensitive() {
    assert_eq!(parse_concept("HOW-IT-WORKS"), Some(Concept::HowItWorks));
    assert_eq!(parse_concept("Pattern"), Some(Concept::Pattern));
}

#[test]
fn test_parse_concept_invalid() {
    assert_eq!(parse_concept("unknown"), None);
    assert_eq!(parse_concept(""), None);
    assert_eq!(parse_concept("random-string"), None);
}
