//! Centralized application configuration loaded once at startup.
//!
//! Eliminates SPOT violation of scattered `env::var()` / `env_parse_with_default()` calls
//! across 14 files. All environment variables are parsed here, validated, and exposed as
//! properly typed fields.

use crate::env_parse_with_default;

/// Centralized application configuration.
///
/// Loaded once at startup from environment variables. Passed as `Arc<AppConfig>`
/// to all services and handlers.
///
/// # Required variables
/// - `DATABASE_URL` — PostgreSQL connection string
/// - `OPENCODE_MEM_API_KEY` or `ANTIGRAVITY_API_KEY` — LLM API key
///
/// # Optional variables (with defaults)
/// See field documentation for env var names and defaults.
#[derive(Debug, Clone)]
pub struct AppConfig {
    // === Required ===
    /// PostgreSQL connection string.
    /// Env: `DATABASE_URL`
    pub database_url: String,

    /// LLM API key.
    /// Env: `OPENCODE_MEM_API_KEY` or `ANTIGRAVITY_API_KEY`
    pub api_key: String,

    // === LLM ===
    /// Base URL for OpenAI-compatible API.
    /// Env: `OPENCODE_MEM_API_URL` (default: `https://api.openai.com`)
    pub api_url: String,

    /// LLM model name for compression.
    /// Env: `OPENCODE_MEM_MODEL` (default: `gpt-4o`)
    pub model: String,

    // === Embeddings ===
    /// Disable vector embeddings entirely.
    /// Env: `OPENCODE_MEM_DISABLE_EMBEDDINGS` (default: `false`, set `1` or `true` to disable)
    pub disable_embeddings: bool,

    /// Number of threads for ONNX embedding inference.
    /// 0 means auto-detect (cores - 1).
    /// Env: `OPENCODE_MEM_EMBEDDING_THREADS` (default: `0` = auto)
    pub embedding_threads: usize,

    // === Infinite Memory ===
    /// Optional separate database URL for infinite memory.
    /// Falls back to `DATABASE_URL` if not set.
    /// Env: `INFINITE_MEMORY_URL` or `OPENCODE_MEM_INFINITE_MEMORY`
    pub infinite_memory_url: Option<String>,

    // === Deduplication ===
    /// Cosine similarity threshold for observation deduplication.
    /// Clamped to `[0.0, 1.0]`.
    /// Env: `OPENCODE_MEM_DEDUP_THRESHOLD` (default: `0.85`)
    pub dedup_threshold: f32,

    /// Cosine similarity threshold for IDE injection loop detection.
    /// Clamped to `[0.0, 1.0]`.
    /// Env: `OPENCODE_MEM_INJECTION_DEDUP_THRESHOLD` (default: `0.80`)
    pub injection_dedup_threshold: f32,

    // === Queue ===
    /// Maximum concurrent queue processing workers.
    /// Env: `OPENCODE_MEM_QUEUE_WORKERS` (default: `10`)
    pub queue_workers: usize,

    /// Maximum retry count for failed queue messages.
    /// Env: `OPENCODE_MEM_MAX_RETRY` (default: `3`)
    pub max_retry: i32,

    /// Visibility timeout in seconds for claimed messages.
    /// Env: `OPENCODE_MEM_VISIBILITY_TIMEOUT` (default: `300`)
    pub visibility_timeout_secs: i64,

    /// Dead letter queue retention in days.
    /// Env: `OPENCODE_MEM_DLQ_TTL_DAYS` (default: `7`)
    pub dlq_ttl_days: i64,

    // === Infinite Memory Compression ===
    /// Maximum characters per event content field before truncation.
    /// Env: `OPENCODE_MEM_MAX_CONTENT_CHARS` (default: `500`)
    pub max_content_chars: usize,

    /// Maximum total characters for LLM compression prompt.
    /// Env: `OPENCODE_MEM_MAX_TOTAL_CHARS` (default: `8000`)
    pub max_total_chars: usize,

    /// Maximum raw events per compression chunk.
    /// Env: `OPENCODE_MEM_MAX_EVENTS` (default: `200`)
    pub max_events: usize,
}

/// Error returned when required configuration is missing or invalid.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A required environment variable is missing.
    #[error("{0}")]
    Missing(String),
}

/// Parse a boolean from env var (accepts `1`, `true` case-insensitive).
fn parse_bool_env(var: &str) -> bool {
    std::env::var(var)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Parse an f32 env var with default, then clamp to `[0.0, 1.0]` with a warning.
fn parse_clamped_threshold(var: &str, default: f32) -> f32 {
    let raw = env_parse_with_default(var, default);
    let clamped = raw.clamp(0.0, 1.0);
    if (clamped - raw).abs() > f32::EPSILON {
        tracing::warn!(
            var,
            original = raw,
            clamped,
            "threshold clamped to [0.0, 1.0]"
        );
    }
    clamped
}

impl AppConfig {
    /// Load configuration from environment variables.
    ///
    /// # Errors
    /// Returns `ConfigError::Missing` if required env vars (`DATABASE_URL`,
    /// `OPENCODE_MEM_API_KEY`/`ANTIGRAVITY_API_KEY`) are not set.
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = std::env::var("DATABASE_URL").map_err(|_| {
            ConfigError::Missing("DATABASE_URL environment variable must be set".to_owned())
        })?;

        let api_key = std::env::var("OPENCODE_MEM_API_KEY")
            .or_else(|_| std::env::var("ANTIGRAVITY_API_KEY"))
            .map_err(|_| {
                ConfigError::Missing(
                    "OPENCODE_MEM_API_KEY or ANTIGRAVITY_API_KEY environment variable must be set"
                        .to_owned(),
                )
            })?;

        let api_url = std::env::var("OPENCODE_MEM_API_URL")
            .unwrap_or_else(|_| "https://api.openai.com".to_owned());

        let model = std::env::var("OPENCODE_MEM_MODEL").unwrap_or_else(|_| "gpt-4o".to_owned());

        let disable_embeddings = parse_bool_env("OPENCODE_MEM_DISABLE_EMBEDDINGS");

        let embedding_threads = env_parse_with_default("OPENCODE_MEM_EMBEDDING_THREADS", 0_usize);

        let infinite_memory_url = std::env::var("INFINITE_MEMORY_URL")
            .or_else(|_| std::env::var("OPENCODE_MEM_INFINITE_MEMORY"))
            .ok();

        let dedup_threshold = parse_clamped_threshold("OPENCODE_MEM_DEDUP_THRESHOLD", 0.85);
        let injection_dedup_threshold =
            parse_clamped_threshold("OPENCODE_MEM_INJECTION_DEDUP_THRESHOLD", 0.80);

        let queue_workers = env_parse_with_default("OPENCODE_MEM_QUEUE_WORKERS", 10_usize);
        let max_retry = env_parse_with_default("OPENCODE_MEM_MAX_RETRY", 3_i32);
        let visibility_timeout_secs =
            env_parse_with_default("OPENCODE_MEM_VISIBILITY_TIMEOUT", 300_i64);
        let dlq_ttl_days = env_parse_with_default("OPENCODE_MEM_DLQ_TTL_DAYS", 7_i64);

        let max_content_chars = env_parse_with_default("OPENCODE_MEM_MAX_CONTENT_CHARS", 500_usize);
        let max_total_chars = env_parse_with_default("OPENCODE_MEM_MAX_TOTAL_CHARS", 8000_usize);
        let max_events = env_parse_with_default("OPENCODE_MEM_MAX_EVENTS", 200_usize);

        Ok(Self {
            database_url,
            api_key,
            api_url,
            model,
            disable_embeddings,
            embedding_threads,
            infinite_memory_url,
            dedup_threshold,
            injection_dedup_threshold,
            queue_workers,
            max_retry,
            visibility_timeout_secs,
            dlq_ttl_days,
            max_content_chars,
            max_total_chars,
            max_events,
        })
    }

    /// Resolved embedding thread count (auto-detect if configured as 0).
    #[must_use]
    pub fn resolved_embedding_threads(&self) -> usize {
        let max_threads = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1);
        if self.embedding_threads == 0 {
            max_threads.saturating_sub(1).max(1)
        } else {
            self.embedding_threads.clamp(1, max_threads)
        }
    }

    /// DLQ TTL in seconds (days × 86400).
    #[must_use]
    pub fn dlq_ttl_secs(&self) -> i64 {
        self.dlq_ttl_days.saturating_mul(86400)
    }
}
