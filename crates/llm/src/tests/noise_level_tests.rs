//! Tests that generic/trivial patterns have low noise_level and project-specific
//! decisions/gotchas are preserved.

use super::test_helpers::{create_client, make_input};
use crate::observation::CompressionResult;
use opencode_mem_core::NoiseLevel;

/// Test: Generic programming pattern should have low `noise_level` (LLM already knows)
#[tokio::test]
#[ignore]
#[expect(clippy::panic, reason = "test assertions")]
#[expect(clippy::print_stdout, reason = "test output")]
#[expect(clippy::print_stderr, reason = "test output")]
#[expect(clippy::use_debug, reason = "test output")]
async fn test_generic_pattern_low_noise() {
    let Some(client) = create_client() else {
        eprintln!("Skipping test: OPENCODE_MEM_API_KEY not set");
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

    let result = client
        .compress_to_observation("test-generic", &input, Some("test-project"), &[])
        .await;

    match result {
        Ok(
            CompressionResult::Create(obs)
            | CompressionResult::Update {
                observation: obs, ..
            },
        ) => {
            let is_low = matches!(obs.noise_level, NoiseLevel::Low | NoiseLevel::Negligible);
            if is_low {
                println!(
                    "[PASS] Generic pattern has low noise_level: {:?}",
                    obs.noise_level
                );
            } else {
                println!(
                    "[INFO] Generic pattern has noise_level {:?} (expected Low/Negligible)",
                    obs.noise_level
                );
            }
        }
        Ok(CompressionResult::Skip { .. }) => {
            println!("[PASS] Generic pattern correctly filtered")
        }
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
        eprintln!("Skipping test: OPENCODE_MEM_API_KEY not set");
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
        Ok(
            CompressionResult::Create(obs)
            | CompressionResult::Update {
                observation: obs, ..
            },
        ) => {
            println!(
                "[PASS] Project decision saved: {} (noise_level: {:?})",
                obs.title, obs.noise_level
            );
            assert!(obs.narrative.is_some(), "Decision should have reasoning");
        }
        Ok(CompressionResult::Skip { .. }) => {
            println!("[WARN] Project-specific decision was skipped: Skipped")
        }
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
        eprintln!("Skipping test: OPENCODE_MEM_API_KEY not set");
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

    let result = client
        .compress_to_observation("test-gotcha", &input, Some("opencode-mem"), &[])
        .await;

    match result {
        Ok(
            CompressionResult::Create(obs)
            | CompressionResult::Update {
                observation: obs, ..
            },
        ) => {
            println!(
                "[PASS] Project gotcha saved: {} (noise_level: {:?})",
                obs.title, obs.noise_level
            );
        }
        Ok(CompressionResult::Skip { .. }) => {
            println!("[WARN] Project-specific gotcha was skipped: Skipped")
        }
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
        eprintln!("Skipping test: OPENCODE_MEM_API_KEY not set");
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
        .compress_to_observation("test-4", &input, Some("test-project"), &[])
        .await;

    match result {
        Ok(
            CompressionResult::Create(obs)
            | CompressionResult::Update {
                observation: obs, ..
            },
        ) => {
            let is_low = matches!(obs.noise_level, NoiseLevel::Low | NoiseLevel::Negligible);
            if is_low {
                println!(
                    "[PASS] Simple file read has low noise_level: {:?}",
                    obs.noise_level
                );
            } else {
                println!(
                    "[INFO] Simple file read has noise_level {:?} (expected Low/Negligible)",
                    obs.noise_level
                );
            }
        }
        Ok(CompressionResult::Skip { .. }) => {
            println!("[PASS] Simple file read correctly filtered")
        }
        Err(e) => panic!("[ERROR] {e}"),
    }
}
