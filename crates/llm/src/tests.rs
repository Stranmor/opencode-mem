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

    /// Test: Trivial "bash executed successfully" should NOT be saved
    #[tokio::test]
    async fn test_trivial_bash_success_not_saved() {
        let Some(client) = create_client() else {
            eprintln!("Skipping test: ANTIGRAVITY_API_KEY not set");
            return;
        };

        let input = make_input(
            "bash",
            "Lists files in current directory",
            r#"{"result":"file1.rs\nfile2.rs\nCargo.toml\n"}"#,
        );

        let result = client
            .compress_to_observation("test-1", &input, Some("test-project"))
            .await;

        match result {
            Ok(None) => println!("✅ PASS: Trivial ls command correctly filtered"),
            Ok(Some(obs)) => panic!("❌ FAIL: Trivial event was saved: {}", obs.title),
            Err(e) => panic!("❌ ERROR: {}", e),
        }
    }

    /// Test: Bugfix with solution SHOULD be saved
    #[tokio::test]
    async fn test_bugfix_with_solution_saved() {
        let Some(client) = create_client() else {
            eprintln!("Skipping test: ANTIGRAVITY_API_KEY not set");
            return;
        };

        let input = make_input(
            "edit",
            "Fixed race condition in auth.rs",
            r#"Edit applied successfully.
            
Changed from:
    let user = cache.get(&id);
    
To:
    let user = cache.read().await.get(&id).cloned();
    
This fixes the race condition where multiple threads could access 
the cache simultaneously without proper synchronization."#,
        );

        let result = client
            .compress_to_observation("test-2", &input, Some("test-project"))
            .await;

        match result {
            Ok(Some(obs)) => {
                println!("✅ PASS: Bugfix saved with title: {}", obs.title);
                assert!(
                    obs.narrative.is_some(),
                    "Bugfix should have narrative explaining the fix"
                );
            }
            Ok(None) => panic!("❌ FAIL: Useful bugfix was incorrectly filtered"),
            Err(e) => panic!("❌ ERROR: {}", e),
        }
    }

    /// Test: Discovery about API behavior SHOULD be saved
    #[tokio::test]
    async fn test_api_discovery_saved() {
        let Some(client) = create_client() else {
            eprintln!("Skipping test: ANTIGRAVITY_API_KEY not set");
            return;
        };

        let input = make_input(
            "bash",
            "Discovered sqlite-vec requires special initialization",
            r#"Error: no such module: vec0

After investigation: sqlite-vec extension must be loaded BEFORE 
creating any connections. The correct order is:
1. Call sqlite_vec::init() 
2. Then open database connection
3. Extension will be available

This is different from other SQLite extensions that can be loaded after."#,
        );

        let result = client
            .compress_to_observation("test-3", &input, Some("test-project"))
            .await;

        match result {
            Ok(Some(obs)) => {
                println!("✅ PASS: API discovery saved: {}", obs.title);
                assert!(!obs.keywords.is_empty(), "Should have searchable keywords");
            }
            Ok(None) => panic!("❌ FAIL: Useful discovery was incorrectly filtered"),
            Err(e) => panic!("❌ ERROR: {}", e),
        }
    }

    /// Test: Simple file read without insight should NOT be saved
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

        let result = client
            .compress_to_observation("test-4", &input, Some("test-project"))
            .await;

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
    assert_eq!(
        strip_markdown_json(r#"{"key": "value"}"#),
        r#"{"key": "value"}"#
    );
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
