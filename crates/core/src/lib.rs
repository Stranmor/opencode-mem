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

/// Strips UUID patterns from text (e.g., `"sshd needs absolute path b3b61de2-..."` → `"sshd needs absolute path"`).
///
/// Handles both full UUIDs (`8-4-4-4-12` hex) and truncated ones (e.g., `b3b61de2-...`).
#[must_use]
pub fn strip_uuid_from_title(title: &str) -> String {
    use std::sync::LazyLock;

    #[expect(
        clippy::unwrap_used,
        reason = "static regex pattern is compile-time validated"
    )]
    static UUID_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r"\s*[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4,}(?:-[0-9a-fA-F]*)*\.{0,3}\s*",
        )
        .unwrap()
    });

    let result = UUID_RE.replace_all(title, " ").trim().to_string();
    if result.is_empty() {
        title.to_string()
    } else {
        result
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_uuid_full() {
        assert_eq!(
            strip_uuid_from_title("sshd needs absolute path b3b61de2-1234-5678-9abc-def012345678"),
            "sshd needs absolute path"
        );
    }

    #[test]
    fn strip_uuid_truncated() {
        assert_eq!(
            strip_uuid_from_title("sshd needs absolute path b3b61de2-1234-5678..."),
            "sshd needs absolute path"
        );
    }

    #[test]
    fn strip_uuid_mid_title() {
        assert_eq!(
            strip_uuid_from_title("Observation b3b61de2-1234-5678-9abc-def012345678 is important"),
            "Observation is important"
        );
    }

    #[test]
    fn strip_uuid_no_uuid() {
        assert_eq!(
            strip_uuid_from_title("normal title without uuid"),
            "normal title without uuid"
        );
    }

    #[test]
    fn strip_uuid_empty() {
        assert_eq!(strip_uuid_from_title(""), "");
    }

    #[test]
    fn strip_uuid_only_uuid() {
        assert_eq!(
            strip_uuid_from_title("b3b61de2-1234-5678-9abc-def012345678"),
            "b3b61de2-1234-5678-9abc-def012345678"
        );
    }

    #[test]
    fn strip_uuid_multiple() {
        assert_eq!(
            strip_uuid_from_title(
                "a b3b61de2-1234-5678-9abc-def012345678 and c4c72ef3-5678-9abc-def0-123456789012"
            ),
            "a and"
        );
    }
}
