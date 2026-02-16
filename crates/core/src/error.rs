//! Typed error enum for the core crate.

use thiserror::Error;

/// Errors originating from core domain type parsing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CoreError {
    /// Failed to parse a session status string.
    #[error("invalid session status: {0}")]
    InvalidSessionStatus(String),
    /// Failed to parse a hook event string.
    #[error("invalid hook event: {0}")]
    InvalidHookEvent(String),
    /// Failed to parse an observation type string.
    #[error("invalid observation type: {0}")]
    InvalidObservationType(String),
    /// Failed to parse a noise level string.
    #[error("invalid noise level: {0}")]
    InvalidNoiseLevel(String),
}
