//! Core types and traits for opencode-mem
//!
//! This crate contains domain types shared across all other crates.

/// Error types for opencode-mem operations.
mod error;
/// Hook event types for IDE integration.
mod hook;
/// JSON utility functions shared across crates.
mod json_utils;
/// Global Knowledge Layer types.
mod knowledge;
/// Observation types for coding session capture.
mod observation;
/// Session types for memory sessions.
mod session;

/// Alias for backward compatibility
pub use error::MemoryError as Error;
pub use error::*;
pub use hook::*;
pub use json_utils::*;
pub use knowledge::*;
pub use observation::*;

pub use session::*;
