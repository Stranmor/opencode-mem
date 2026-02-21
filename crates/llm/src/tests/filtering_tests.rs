use std::env;

use crate::client::LlmClient;
use crate::observation::CompressionResult;
use opencode_mem_core::{NoiseLevel, Observation, ObservationInput, ObservationType, ToolOutput};

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
        Ok(CompressionResult::Create(obs) | CompressionResult::Update { observation: obs, .. }) => {
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
        Ok(CompressionResult::Skip { .. }) => {
            println!("[PASS] Generic pattern correctly filtered")
        },
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

    let result =
        client.compress_to_observation("test-decision", &input, Some("opencode-mem"), &[]).await;

    match result {
        Ok(CompressionResult::Create(obs) | CompressionResult::Update { observation: obs, .. }) => {
            println!(
                "[PASS] Project decision saved: {} (noise_level: {:?})",
                obs.title, obs.noise_level
            );
            assert!(obs.narrative.is_some(), "Decision should have reasoning");
        },
        Ok(CompressionResult::Skip { .. }) => {
            println!("[WARN] Project-specific decision was skipped: Skipped")
        },
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
        Ok(CompressionResult::Create(obs) | CompressionResult::Update { observation: obs, .. }) => {
            println!(
                "[PASS] Project gotcha saved: {} (noise_level: {:?})",
                obs.title, obs.noise_level
            );
        },
        Ok(CompressionResult::Skip { .. }) => {
            println!("[WARN] Project-specific gotcha was skipped: Skipped")
        },
        Err(e) => panic!("[ERROR] {e}"),
    }
}

/// Test: Duplicate observation should be marked negligible when similar candidates provided
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

    // Candidate observation covering the same topic as the input
    let candidate = Observation::builder(
        "existing-proxy-fix".to_owned(),
        "session-1".to_owned(),
        ObservationType::Bugfix,
        "Fixed proxy client to return Result instead of fallback".to_owned(),
    )
    .maybe_narrative(Some(
        "The proxy client was silently falling back to a direct connection \
             when the proxy URL was invalid. This caused IP leaks. \
             Fixed by returning Result from create_client_with_proxy."
            .to_owned(),
    ))
    .build();

    // Simulate the same observation arriving again
    let input = make_input(
        "bash",
        "Fixed proxy client to return Result instead of fallback",
        "The proxy client was silently falling back to a direct connection \
             when the proxy URL was invalid. This caused IP leaks. \
             Fixed by returning Result from create_client_with_proxy.",
    );

    let result = client
        .compress_to_observation("test-dedup", &input, Some("test-project"), &[candidate])
        .await;

    match result {
        Ok(CompressionResult::Create(obs) | CompressionResult::Update { observation: obs, .. }) => {
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
        Ok(CompressionResult::Skip { .. }) => {
            println!("[PASS] Duplicate correctly filtered out")
        },
        Err(e) => panic!("[ERROR] {e}"),
    }
}

/// Test: Genuinely new insight should still be saved even with unrelated candidates
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

    let unrelated_candidate = Observation::builder(
        "existing-proxy-fix".to_owned(),
        "session-1".to_owned(),
        ObservationType::Bugfix,
        "Fixed proxy client to return Result instead of fallback".to_owned(),
    )
    .maybe_narrative(Some(
        "The proxy client was silently falling back to a direct connection \
             when the proxy URL was invalid. This caused IP leaks."
            .to_owned(),
    ))
    .build();

    let input = make_input(
        "bash",
        "Discovered SQLite WAL mode requires shared-memory for concurrent readers",
        "After switching to WAL mode, concurrent readers from different \
             processes failed with SQLITE_BUSY. Root cause: WAL requires \
             shared memory (-shm file) which doesn't work on network filesystems. \
             Fix: use journal_mode=DELETE for network mounts.",
    );

    let result = client
        .compress_to_observation("test-new", &input, Some("test-project"), &[unrelated_candidate])
        .await;

    match result {
        Ok(CompressionResult::Create(obs) | CompressionResult::Update { observation: obs, .. }) => {
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
        Ok(CompressionResult::Skip { .. }) => {
            panic!("[FAIL] New insight was incorrectly filtered out")
        },
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

    let result = client.compress_to_observation("test-4", &input, Some("test-project"), &[]).await;

    match result {
        Ok(CompressionResult::Create(obs) | CompressionResult::Update { observation: obs, .. }) => {
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
        Ok(CompressionResult::Skip { .. }) => {
            println!("[PASS] Simple file read correctly filtered")
        },
        Err(e) => panic!("[ERROR] {e}"),
    }
}

/// Test: LLM should skip/update when candidates are nearly identical to input
#[tokio::test]
#[ignore]
#[expect(clippy::panic, reason = "test assertions")]
#[expect(clippy::print_stdout, reason = "test output")]
#[expect(clippy::print_stderr, reason = "test output")]
#[expect(clippy::use_debug, reason = "test output")]
async fn test_context_aware_skip_with_duplicate_candidates() {
    let Some(client) = create_client() else {
        eprintln!("Skipping test: ANTIGRAVITY_API_KEY not set");
        return;
    };

    let candidate = Observation::builder(
        "obs-advisory-lock".to_owned(),
        "session-prev".to_owned(),
        ObservationType::Bugfix,
        "Advisory lock leak on connection drop — fixed with after_release hook".to_owned(),
    )
    .maybe_narrative(Some(
        "PostgreSQL advisory locks were not released when the connection pool \
             recycled connections. The lock stayed held until the backend process \
             terminated. Fixed by adding an after_release callback that explicitly \
             calls pg_advisory_unlock_all()."
            .to_owned(),
    ))
    .keywords(vec![
        "advisory lock".to_owned(),
        "connection pool".to_owned(),
        "pg_advisory_unlock_all".to_owned(),
    ])
    .build();

    let input = make_input(
        "bash",
        "Advisory lock leak — connection pool doesn't release locks",
        "PostgreSQL advisory locks were leaking because the connection pool \
             recycled connections without releasing them. The lock persisted until \
             the backend process died. Solution: after_release hook that calls \
             pg_advisory_unlock_all().",
    );

    let result = client
        .compress_to_observation("test-skip-dup", &input, Some("test-project"), &[candidate])
        .await;

    match result {
        Ok(CompressionResult::Skip { reason }) => {
            println!("[PASS] Duplicate correctly skipped: {reason}");
        },
        Ok(CompressionResult::Update { target_id, .. }) => {
            println!("[PASS] Duplicate correctly merged into existing: {target_id}");
        },
        Ok(CompressionResult::Create(obs)) => {
            println!(
                "[WARN] Expected skip/update but got create: {} ({:?})",
                obs.title, obs.noise_level
            );
        },
        Err(e) => panic!("[ERROR] {e}"),
    }
}

/// Test: LLM should create when candidates are completely unrelated to input
#[tokio::test]
#[ignore]
#[expect(clippy::panic, reason = "test assertions")]
#[expect(clippy::print_stdout, reason = "test output")]
#[expect(clippy::print_stderr, reason = "test output")]
#[expect(clippy::use_debug, reason = "test output")]
async fn test_context_aware_create_with_unrelated_candidates() {
    let Some(client) = create_client() else {
        eprintln!("Skipping test: ANTIGRAVITY_API_KEY not set");
        return;
    };

    let unrelated_candidate = Observation::builder(
        "obs-css-grid".to_owned(),
        "session-other".to_owned(),
        ObservationType::Gotcha,
        "CSS Grid auto-fit creates implicit tracks that break min-height".to_owned(),
    )
    .maybe_narrative(Some(
        "Using auto-fit with minmax() in CSS Grid created implicit row tracks \
             that ignored the container's min-height constraint. Fixed by switching \
             to explicit grid-template-rows."
            .to_owned(),
    ))
    .build();

    let input = make_input(
        "bash",
        "PostgreSQL NOTIFY payload limited to 8000 bytes",
        "Discovered that pg_notify() silently truncates payloads exceeding \
             8000 bytes. Our JSON event payload was 12KB and was being silently \
             cut off. Fixed by storing the payload in a table and sending only \
             the row ID via NOTIFY.",
    );

    let result = client
        .compress_to_observation(
            "test-create-unrelated",
            &input,
            Some("test-project"),
            &[unrelated_candidate],
        )
        .await;

    match result {
        Ok(CompressionResult::Create(obs)) => {
            let is_saved = !matches!(obs.noise_level, NoiseLevel::Negligible);
            if is_saved {
                println!(
                    "[PASS] New insight created despite unrelated candidates: {} ({:?})",
                    obs.title, obs.noise_level
                );
            } else {
                panic!("[FAIL] New insight incorrectly marked negligible: {}", obs.title);
            }
        },
        Ok(CompressionResult::Update { target_id, .. }) => {
            panic!("[FAIL] New unrelated insight should not update existing: {target_id}");
        },
        Ok(CompressionResult::Skip { reason }) => {
            panic!("[FAIL] New unrelated insight should not be skipped: {reason}");
        },
        Err(e) => panic!("[ERROR] {e}"),
    }
}
