//! OpenCode plugin integration
//!
//! Note: The actual plugin is TypeScript (required by OpenCode).
//! This crate contains Rust types/logic that the TS plugin calls via HTTP.

pub mod context;

pub use context::*;
