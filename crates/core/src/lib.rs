//! Core types and traits for opencode-mem
//!
//! This crate contains domain types shared across all other crates.

/// Shared constants (pool sizes, query limits, etc.).
mod constants;
/// Environment variable parsing with warn-level logging.
mod env_config;
/// Hook event types for IDE integration.
mod hook;
/// JSON utility functions shared across crates.
mod json_utils;
/// Global Knowledge Layer types.
mod knowledge;
/// Observation types for coding session capture.
mod observation;
/// Project filtering based on glob patterns from env.
mod project_filter;
/// Session types for memory sessions.
mod session;

pub use constants::*;
pub use env_config::*;
pub use hook::*;
pub use json_utils::*;
pub use knowledge::*;
pub use observation::*;
pub use project_filter::*;
pub use session::*;
