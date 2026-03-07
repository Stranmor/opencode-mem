//! Core types and traits for opencode-mem
//!
//! This crate contains domain types shared across all other crates.

mod app_config;
mod constants;
mod env_config;
pub mod error;
mod hook;
mod identifiers;
pub mod infinite_memory;
mod json_utils;
mod knowledge;
mod observation;
mod project_filter;
mod session;

pub use app_config::*;
pub use constants::*;
pub use env_config::*;
pub use error::CoreError;
pub use hook::*;
pub use identifiers::*;
pub use infinite_memory::{
    InfiniteEventType, InfiniteSummary, RawInfiniteEvent, StoredInfiniteEvent, SummaryEntities,
    assistant_event, tool_event, user_event,
};
pub use json_utils::*;
pub use knowledge::*;
pub use observation::*;
pub use project_filter::*;
pub use session::*;

/// Truncates a string to the given maximum length at a char boundary.
#[must_use]
pub fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        s.get(..end).unwrap_or("")
    }
}
