use std::env;

use crate::client::truncate;
use crate::observation::parse_concept;
use opencode_mem_core::{strip_markdown_json, Concept, NoiseLevel, ObservationInput, ToolOutput};

// Integration tests for observation filtering (require ANTIGRAVITY_API_KEY)
#[cfg(test)]
mod filtering_tests {
    use super::*;
    use crate::client::LlmClient;

    fn create_client() -> Option<LlmClient> {
        let api_key = env::var("ANTIGRAVITY_API_KEY").ok()?;
        Some(
            LlmClient::new(api_key, "https://antigravity.quantumind.ru".to_owned())
                .ok()?
                .with_model("gemini-3-flash".to_owned()),
        )
    }

    fn make_input(tool: &str, title: &str, output: &str) -> ObservationInput {
        ObservationInput::new(
            tool.to_owned(),
            "test-session".to_owned(),
            format!("test-call-{}", uuid::Uuid::new_v4()),
            ToolOutput::new(title.to_owned(), output.to_owned(), serde_json::Value::Null),
        )
    }

    /// Test: Generic programming pattern should have low `noise_level` (LLM already knows)
    #[tokio::test]
    #[ignore]
    #[expect(clippy::panic, reason = "test assertions")]
    #[expect(clippy::print_stdout, reason = "test output")]
    #[expect(clippy::print_stderr, reason = "test output")]
    #[expect(clippy::use_debug, reason = "test output")]
    async fn test_generic_pattern_low_noise() {
        let Some(client) = create_client() else {
            eprintln!("Skipping test: ANTIGRAVITY_API_KEY not set");
            return;
        };

        let input = make_input(
            "edit",
            "Fixed race condition using RwLock",
            "Changed from:
    let user = cache.get(&id);
To:
    let user = cache.read().await.get(&id).cloned();
    
Standard fix for race condition - use RwLock instead of direct access.",
        );

        let result =
            client.compress_to_observation("test-generic", &input, Some("test-project"), &[]).await;

        match result {
            Ok(Some(obs)) => {
                // Generic patterns should have Low or Negligible noise_level
                let is_low = matches!(obs.noise_level, NoiseLevel::Low | NoiseLevel::Negligible);
                if is_low {
                    println!("[PASS] Generic pattern has low noise_level: {:?}", obs.noise_level);
                } else {
                    println!(
                        "[INFO] Generic pattern has noise_level {:?} (expected Low/Negligible)",
                        obs.noise_level
                    );
                }
            },
            Ok(None) => println!("[PASS] Generic pattern correctly filtered"),
            Err(e) => panic!("[ERROR] {e}"),
        }
    }

    /// Test: Project-specific decision SHOULD be saved with high `noise_level`
    #[tokio::test]
    #[ignore]
    #[expect(clippy::panic, reason = "test assertions")]
    #[expect(clippy::print_stdout, reason = "test output")]
    #[expect(clippy::print_stderr, reason = "test output")]
    #[expect(clippy::use_debug, reason = "test output")]
    async fn test_project_decision_saved() {
        let Some(client) = create_client() else {
            eprintln!("Skipping test: ANTIGRAVITY_API_KEY not set");
            return;
        };

        let input = make_input(
            "edit",
            "Architecture decision: chose pgvector over ChromaDB",
            "Decision for opencode-mem project:

We chose pgvector instead of ChromaDB because:
1. Single database - PostgreSQL handles both relational and vector data
2. No Python dependency - simpler ops for CLI tool
3. pgvector supports cosine similarity, L2, and inner product

Trade-off: ChromaDB has a nicer API, but we prioritize infrastructure simplicity.",
        );

        let result = client
            .compress_to_observation("test-decision", &input, Some("opencode-mem"), &[])
            .await;

        match result {
            Ok(Some(obs)) => {
                println!(
                    "[PASS] Project decision saved: {} (noise_level: {:?})",
                    obs.title, obs.noise_level
                );
                assert!(obs.narrative.is_some(), "Decision should have reasoning");
            },
            Ok(None) => panic!("[FAIL] Project-specific decision was incorrectly filtered"),
            Err(e) => panic!("[ERROR] {e}"),
        }
    }

    /// Test: Project-specific gotcha SHOULD be saved
    #[tokio::test]
    #[ignore]
    #[expect(clippy::panic, reason = "test assertions")]
    #[expect(clippy::print_stdout, reason = "test output")]
    #[expect(clippy::print_stderr, reason = "test output")]
    #[expect(clippy::use_debug, reason = "test output")]
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
            client.compress_to_observation("test-gotcha", &input, Some("opencode-mem"), &[]).await;

        match result {
            Ok(Some(obs)) => {
                println!(
                    "[PASS] Project gotcha saved: {} (noise_level: {:?})",
                    obs.title, obs.noise_level
                );
            },
            Ok(None) => panic!("[FAIL] Project-specific gotcha was incorrectly filtered"),
            Err(e) => panic!("[ERROR] {e}"),
        }
    }

    /// Test: Duplicate observation should be marked negligible when existing titles provided
    #[tokio::test]
    #[ignore]
    #[expect(clippy::panic, reason = "test assertions")]
    #[expect(clippy::print_stdout, reason = "test output")]
    #[expect(clippy::print_stderr, reason = "test output")]
    #[expect(clippy::use_debug, reason = "test output")]
    async fn test_duplicate_marked_negligible_with_context() {
        let Some(client) = create_client() else {
            eprintln!("Skipping test: ANTIGRAVITY_API_KEY not set");
            return;
        };

        // Simulate an observation about a proxy IP leak — same topic as existing
        let input = make_input(
            "bash",
            "Fixed proxy client to return Result instead of fallback",
            "The proxy client was silently falling back to a direct connection \
             when the proxy URL was invalid. This caused IP leaks. \
             Fixed by returning Result from create_client_with_proxy.",
        );

        let existing_titles = vec![
            "Invalid proxy configurations caused silent IP leaks".to_owned(),
            "Proxy client creation must return Result to prevent silent IP leaks".to_owned(),
        ];

        let result = client
            .compress_to_observation("test-dedup", &input, Some("test-project"), &existing_titles)
            .await;

        match result {
            Ok(Some(obs)) => {
                let is_negligible = matches!(obs.noise_level, NoiseLevel::Negligible);
                if is_negligible {
                    println!("[PASS] Duplicate correctly marked negligible: {:?}", obs.noise_level);
                } else {
                    println!(
                        "[WARN] Duplicate was NOT marked negligible: {:?} (expected Negligible)",
                        obs.noise_level
                    );
                }
            },
            Ok(None) => println!("[PASS] Duplicate correctly filtered out"),
            Err(e) => panic!("[ERROR] {e}"),
        }
    }

    /// Test: Genuinely new insight should still be saved even with existing titles
    #[tokio::test]
    #[ignore]
    #[expect(clippy::panic, reason = "test assertions")]
    #[expect(clippy::print_stdout, reason = "test output")]
    #[expect(clippy::print_stderr, reason = "test output")]
    #[expect(clippy::use_debug, reason = "test output")]
    async fn test_new_insight_saved_despite_existing_titles() {
        let Some(client) = create_client() else {
            eprintln!("Skipping test: ANTIGRAVITY_API_KEY not set");
            return;
        };

        // New insight about a completely different topic
        let input = make_input(
            "bash",
            "Discovered SQLite WAL mode requires shared-memory for concurrent readers",
            "After switching to WAL mode, concurrent readers from different \
             processes failed with SQLITE_BUSY. Root cause: WAL requires \
             shared memory (-shm file) which doesn't work on network filesystems. \
             Fix: use journal_mode=DELETE for network mounts.",
        );

        let existing_titles = vec![
            "Invalid proxy configurations caused silent IP leaks".to_owned(),
            "Gemini API effort settings must be in thinkingConfig".to_owned(),
        ];

        let result = client
            .compress_to_observation("test-new", &input, Some("test-project"), &existing_titles)
            .await;

        match result {
            Ok(Some(obs)) => {
                let is_saved = !matches!(obs.noise_level, NoiseLevel::Negligible);
                if is_saved {
                    println!(
                        "[PASS] New insight saved despite existing titles: {} ({:?})",
                        obs.title, obs.noise_level
                    );
                } else {
                    panic!("[FAIL] New insight was incorrectly marked negligible: {}", obs.title);
                }
            },
            Ok(None) => panic!("[FAIL] New insight was incorrectly filtered out"),
            Err(e) => panic!("[ERROR] {e}"),
        }
    }

    /// Test: Simple file read should have low `noise_level`
    #[tokio::test]
    #[ignore]
    #[expect(clippy::panic, reason = "test assertions")]
    #[expect(clippy::print_stdout, reason = "test output")]
    #[expect(clippy::print_stderr, reason = "test output")]
    #[expect(clippy::use_debug, reason = "test output")]
    async fn test_simple_file_read_low_noise() {
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

        let result =
            client.compress_to_observation("test-4", &input, Some("test-project"), &[]).await;

        match result {
            Ok(Some(obs)) => {
                // Simple file reads should have Low or Negligible noise_level
                let is_low = matches!(obs.noise_level, NoiseLevel::Low | NoiseLevel::Negligible);
                if is_low {
                    println!("[PASS] Simple file read has low noise_level: {:?}", obs.noise_level);
                } else {
                    println!(
                        "[INFO] Simple file read has noise_level {:?} (expected Low/Negligible)",
                        obs.noise_level
                    );
                }
            },
            Ok(None) => println!("[PASS] Simple file read correctly filtered"),
            Err(e) => panic!("[ERROR] {e}"),
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
    // "privet" in cyrillic: \u043f\u0440\u0438\u0432\u0435\u0442
    let s = "\u{043f}\u{0440}\u{0438}\u{0432}\u{0435}\u{0442}";
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
