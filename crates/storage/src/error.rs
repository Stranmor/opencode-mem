//! Typed error enum for the storage layer.
//!
//! Replaces `anyhow::Result` in all storage traits and implementations,
//! enabling callers to match on specific failure modes (not found, duplicate,
//! transient DB errors) instead of downcasting opaque boxes.

use thiserror::Error;

/// Storage-layer error with variants covering every expected failure mode.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Row not found for expected-present entity.
    #[error("not found: {entity} with id {id}")]
    NotFound { entity: &'static str, id: String },

    /// Unique constraint violation (dedup, title collision).
    #[error("duplicate: {0}")]
    Duplicate(String),

    /// SQL / connection / timeout failure.
    #[error("database error: {0}")]
    Database(#[source] sqlx::Error),

    /// Row data could not be deserialized into domain type.
    #[error("data corruption: {context}")]
    DataCorruption {
        context: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Migration failure.
    #[error("migration error: {0}")]
    Migration(String),

    /// Database is unavailable (circuit breaker open).
    /// Callers should return empty results for reads, skip for writes.
    #[error("database unavailable (circuit breaker open, next probe in {seconds_until_probe}s)")]
    Unavailable { seconds_until_probe: u64 },
}

impl StorageError {
    /// Whether this error is a connection/availability failure (worth retrying).
    ///
    /// Matches all sqlx error variants that indicate the database is unreachable
    /// or the connection pool is exhausted — NOT query-level errors like constraint
    /// violations or row-not-found.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Database(
                sqlx::Error::PoolTimedOut
                | sqlx::Error::PoolClosed
                | sqlx::Error::WorkerCrashed
                | sqlx::Error::Io(_),
            ) => true,
            // sqlx wraps connection-refused/no-route-to-host as Database(BoxDynError)
            // when the error comes from the connection layer. Check the error message
            // for common connection failure patterns.
            Self::Database(sqlx::Error::Database(db_err)) => {
                let msg = db_err.message();
                msg.contains("connection refused")
                    || msg.contains("No route to host")
                    || msg.contains("Connection reset")
                    || msg.contains("broken pipe")
            },
            _ => false,
        }
    }

    /// Whether this error is a unique-constraint violation.
    pub fn is_duplicate(&self) -> bool {
        matches!(self, Self::Duplicate(_))
    }

    /// Whether this error indicates the database is completely unavailable.
    ///
    /// Returns `true` for both the explicit `Unavailable` variant (circuit breaker open)
    /// and for connection-level failures detected via `is_transient()`.
    pub fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable { .. }) || self.is_transient()
    }
}

/// Custom `From<sqlx::Error>` — NOT blanket `#[from]`.
///
/// - `RowNotFound` → `NotFound` (generic; callers should catch and remap with entity context)
/// - SQLSTATE 23505 → `Duplicate`
/// - Everything else → `Database`
impl From<sqlx::Error> for StorageError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => Self::NotFound { entity: "row", id: "unknown".into() },
            sqlx::Error::Database(db_err) if db_err.code().is_some_and(|c| c == "23505") => {
                Self::Duplicate(db_err.message().to_owned())
            },
            _ => Self::Database(err),
        }
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(err: serde_json::Error) -> Self {
        Self::DataCorruption {
            context: "JSON serialization/deserialization".to_owned(),
            source: Box::new(err),
        }
    }
}
