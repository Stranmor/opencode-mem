//! `OpenCode` plugin integration
//!
//! Note: The actual plugin is TypeScript (required by `OpenCode`).
//! This crate contains Rust types/logic that the TS plugin calls via HTTP.

/// Context building for memory injection.
pub mod context;
/// Hook events and context for plugin integration.
pub mod hooks;

pub use context::*;
pub use hooks::*;
