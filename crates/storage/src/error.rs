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
}

impl StorageError {
    /// Whether this error is likely transient (worth retrying).
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Database(sqlx::Error::PoolTimedOut | sqlx::Error::Io(_)))
    }

    /// Whether this error is a unique-constraint violation.
    pub fn is_duplicate(&self) -> bool {
        matches!(self, Self::Duplicate(_))
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
