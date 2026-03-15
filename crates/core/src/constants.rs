//! Shared constants for opencode-mem.
//!
//! Centralizes magic numbers that were previously duplicated across crates.

/// Maximum number of results for any query (DoS protection).
pub const MAX_QUERY_LIMIT: usize = 1000;

/// Maximum number of results for any query as i64 (for MCP handlers).
pub const MAX_QUERY_LIMIT_I64: i64 = 1000;

/// PostgreSQL connection pool: maximum connections.
pub const PG_POOL_MAX_CONNECTIONS: u32 = 20;

/// PostgreSQL connection pool: acquire timeout in seconds.
/// Short timeout (3s) ensures fast-fail when DB is unavailable,
/// triggering circuit breaker instead of hanging for 30+ seconds.
pub const PG_POOL_ACQUIRE_TIMEOUT_SECS: u64 = 3;

/// PostgreSQL connection pool: idle timeout in seconds.
pub const PG_POOL_IDLE_TIMEOUT_SECS: u64 = 300;

/// Maximum number of IDs in a batch request (DoS protection).
pub const MAX_BATCH_IDS: usize = 500;

/// Default number of results when limit is not specified by the caller.
pub const DEFAULT_QUERY_LIMIT: usize = 20;

/// Error message when Infinite Memory is not configured.
pub const INFINITE_MEMORY_NOT_CONFIGURED: &str =
    "Infinite Memory not configured (INFINITE_MEMORY_URL not set)";

/// Embedding vector dimension (BGE-M3 model: 1024d, 100+ languages).
pub const EMBEDDING_DIMENSION: usize = 1024;

/// Maximum observations to load for background dedup sweep.
/// Internal use only (not exposed to API). O(N²) comparison is bounded
/// by this limit — at 5000 observations, sweep processes ~12.5M pairs
/// in a spawn_blocking thread (~2-3 seconds on modern CPU).
pub const DEDUP_SWEEP_MAX_OBSERVATIONS: usize = 5000;

/// Trigram similarity threshold for merging knowledge entries.
/// Titles with similarity above this are considered duplicates and merged.
pub const KNOWLEDGE_TRIGRAM_MERGE_THRESHOLD: f32 = 0.7;

/// Trigram similarity threshold for logging near-misses.
/// Titles with similarity between this and `KNOWLEDGE_TRIGRAM_MERGE_THRESHOLD`
/// are logged at info level for monitoring but not merged.
pub const KNOWLEDGE_TRIGRAM_LOG_THRESHOLD: f32 = 0.5;

/// Maximum number of trigram similarity candidates to consider.
pub const KNOWLEDGE_TRIGRAM_CANDIDATE_LIMIT: i64 = 3;

/// Cosine similarity threshold for semantic dedup of knowledge entries.
/// Knowledge entries with embedding similarity above this are merged.
/// Higher than observation dedup (0.85) because knowledge titles are shorter
/// and semantically denser — false positives are costlier.
pub const KNOWLEDGE_SEMANTIC_DEDUP_THRESHOLD: f32 = 0.85;

/// Cap a user-supplied query limit to `MAX_QUERY_LIMIT`.
///
/// Both HTTP and MCP transports need to clamp user-supplied limits for DoS
/// protection. This is the single point of truth (SPOT) — transport-specific
/// `capped_limit()` methods and `parse_limit()` delegate here.
#[must_use]
pub const fn cap_query_limit(limit: usize) -> usize {
    if limit < MAX_QUERY_LIMIT {
        limit
    } else {
        MAX_QUERY_LIMIT
    }
}
