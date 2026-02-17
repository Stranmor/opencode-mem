//! Shared constants for opencode-mem.
//!
//! Centralizes magic numbers that were previously duplicated across crates.

/// Maximum number of results for any query (DoS protection).
pub const MAX_QUERY_LIMIT: usize = 1000;

/// Maximum number of results for any query as i64 (for MCP handlers).
pub const MAX_QUERY_LIMIT_I64: i64 = 1000;

/// PostgreSQL connection pool: maximum connections.
pub const PG_POOL_MAX_CONNECTIONS: u32 = 8;

/// PostgreSQL connection pool: acquire timeout in seconds.
pub const PG_POOL_ACQUIRE_TIMEOUT_SECS: u64 = 10;

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
