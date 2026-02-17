//! Observation types for coding session capture.

mod builder;
mod input;
mod low_value_filter;

mod dedup;
#[cfg(test)]
mod dedup_tests;
mod merge;

pub use builder::*;
pub use dedup::*;
pub use input::*;
pub use low_value_filter::is_low_value_observation;
pub use merge::*;

use std::fmt;
use std::str::FromStr;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Ordinal position of a prompt within a session.
///
/// Semantically distinct from token counts or other numeric identifiers —
/// wrapping in a newtype prevents accidental swaps at construction sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PromptNumber(pub u32);

/// Token count for ROI (return on investment) tracking.
///
/// Semantically distinct from prompt ordinals or other numeric fields —
/// wrapping in a newtype prevents accidental swaps at construction sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiscoveryTokens(pub u32);

impl From<u32> for PromptNumber {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<PromptNumber> for u32 {
    fn from(v: PromptNumber) -> Self {
        v.0
    }
}

impl fmt::Display for PromptNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<u32> for DiscoveryTokens {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<DiscoveryTokens> for u32 {
    fn from(v: DiscoveryTokens) -> Self {
        v.0
    }
}

impl fmt::Display for DiscoveryTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Regex pattern for matching private content tags.
#[expect(clippy::unwrap_used, reason = "static regex pattern is compile-time validated")]
static PRIVATE_TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(?is)<private>.*?</private>").unwrap());

/// Filters out content wrapped in `<private>...</private>` tags.
pub fn filter_private_content(text: &str) -> String {
    PRIVATE_TAG_REGEX.replace_all(text, "").into_owned()
}

/// Regex pattern for matching injected memory blocks.
/// Matches `<memory-global>...</memory-global>` and similar memory injection tags
/// like `<memory-project>`, `<memory-session>`, etc.
/// Also handles optional XML attributes on the opening tag.
#[expect(clippy::unwrap_used, reason = "static regex pattern is compile-time validated")]
static MEMORY_TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<memory-[a-z][^>]*>.*?</memory-[^>]+>").unwrap());

/// Regex for unclosed memory tags (truncation safety).
/// Strips from opening tag to end-of-string when no closing tag exists.
#[expect(clippy::unwrap_used, reason = "static regex pattern is compile-time validated")]
static MEMORY_UNCLOSED_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<memory-[a-z][^>]*>.*$").unwrap());

/// Strips injected memory blocks (`<memory-*>...</memory-*>`) from text.
///
/// Memory blocks are injected into conversation context by the IDE plugin.
/// Without filtering, the observe hook re-processes them, creating duplicate
/// observations that get re-injected — causing infinite recursion.
///
/// Handles both well-formed tags and unclosed tags (e.g. from truncation).
pub fn filter_injected_memory(text: &str) -> String {
    let after_closed = MEMORY_TAG_REGEX.replace_all(text, "");
    MEMORY_UNCLOSED_REGEX.replace_all(&after_closed, "").into_owned()
}

/// Type of observation captured during a coding session
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ObservationType {
    /// Bug fix observation
    Bugfix,
    /// New feature implementation
    Feature,
    /// Code refactoring
    Refactor,
    /// General code change
    Change,
    /// Discovery about codebase or API
    Discovery,
    /// Architectural or design decision
    Decision,
    /// Gotcha or pitfall to remember
    Gotcha,
    /// User preference or workflow
    Preference,
}

impl ObservationType {
    /// Returns the string representation of the observation type.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match *self {
            Self::Bugfix => "bugfix",
            Self::Feature => "feature",
            Self::Refactor => "refactor",
            Self::Change => "change",
            Self::Discovery => "discovery",
            Self::Decision => "decision",
            Self::Gotcha => "gotcha",
            Self::Preference => "preference",
        }
    }
}

impl FromStr for ObservationType {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bugfix" => Ok(Self::Bugfix),
            "feature" => Ok(Self::Feature),
            "refactor" => Ok(Self::Refactor),
            "change" => Ok(Self::Change),
            "discovery" => Ok(Self::Discovery),
            "decision" => Ok(Self::Decision),
            "gotcha" => Ok(Self::Gotcha),
            "preference" => Ok(Self::Preference),
            other => Err(CoreError::InvalidObservationType(other.to_owned())),
        }
    }
}

/// Semantic concepts for observation categorization
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Concept {
    /// Explains how something works internally
    HowItWorks,
    /// Explains why something exists or was designed this way
    WhyItExists,
    /// Documents what changed
    WhatChanged,
    /// Problem and its solution
    ProblemSolution,
    /// Gotcha or pitfall
    Gotcha,
    /// Reusable pattern
    Pattern,
    /// Trade-off between alternatives
    TradeOff,
}

impl Concept {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match *self {
            Self::HowItWorks => "how-it-works",
            Self::WhyItExists => "why-it-exists",
            Self::WhatChanged => "what-changed",
            Self::ProblemSolution => "problem-solution",
            Self::Gotcha => "gotcha",
            Self::Pattern => "pattern",
            Self::TradeOff => "trade-off",
        }
    }
}

/// Signal vs noise classification for observations.
/// Critical = must always show, Negligible = hide by default.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum NoiseLevel {
    /// Must always be shown - critical project knowledge
    Critical,
    /// Important observation - show by default
    High,
    /// Standard observation - show by default
    #[default]
    Medium,
    /// Minor observation - hide by default
    Low,
    /// Routine/noisy - hide by default
    Negligible,
}

impl NoiseLevel {
    /// Returns the string representation of the noise level.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match *self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Negligible => "negligible",
        }
    }
}

impl FromStr for NoiseLevel {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "critical" => Ok(Self::Critical),
            "high" => Ok(Self::High),
            "medium" => Ok(Self::Medium),
            "low" => Ok(Self::Low),
            "negligible" => Ok(Self::Negligible),
            other => Err(CoreError::InvalidNoiseLevel(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_private_simple() {
        let input = "Hello <private>secret</private> world";
        assert_eq!(filter_private_content(input), "Hello  world");
    }

    #[test]
    fn filter_private_multiline() {
        let input = "Start\n<private>\nSecret data\n</private>\nEnd";
        assert_eq!(filter_private_content(input), "Start\n\nEnd");
    }

    #[test]
    fn filter_private_case_insensitive() {
        let input = "Hello <PRIVATE>secret</PRIVATE> world";
        assert_eq!(filter_private_content(input), "Hello  world");
    }

    #[test]
    fn filter_private_multiple_tags() {
        let input = "A <private>x</private> B <private>y</private> C";
        assert_eq!(filter_private_content(input), "A  B  C");
    }

    #[test]
    fn filter_private_no_tags() {
        let input = "No private content here";
        assert_eq!(filter_private_content(input), "No private content here");
    }

    #[test]
    fn filter_private_empty_tag() {
        let input = "Hello <private></private> world";
        assert_eq!(filter_private_content(input), "Hello  world");
    }

    #[test]
    fn filter_private_nested_content() {
        let input = "Data <private>API_KEY=sk-12345\nPASSWORD=hunter2</private> end";
        assert_eq!(filter_private_content(input), "Data  end");
    }

    #[test]
    fn filter_memory_global() {
        let input = "Normal text\n<memory-global>\n- [gotcha] Some memory\n- [decision] Another\n</memory-global>\nMore text";
        assert_eq!(filter_injected_memory(input), "Normal text\n\nMore text");
    }

    #[test]
    fn filter_memory_multiple_tags() {
        let input = "A <memory-global>x</memory-global> B <memory-project>y</memory-project> C";
        assert_eq!(filter_injected_memory(input), "A  B  C");
    }

    #[test]
    fn filter_memory_case_insensitive() {
        let input = "Hello <MEMORY-GLOBAL>data</MEMORY-GLOBAL> world";
        assert_eq!(filter_injected_memory(input), "Hello  world");
    }

    #[test]
    fn filter_memory_no_tags() {
        let input = "No memory tags here";
        assert_eq!(filter_injected_memory(input), "No memory tags here");
    }

    #[test]
    fn filter_memory_multiline_content() {
        let input = "Start\n<memory-global>\n- line 1\n- line 2\n- line 3\n</memory-global>\nEnd";
        assert_eq!(filter_injected_memory(input), "Start\n\nEnd");
    }

    #[test]
    fn filter_memory_preserves_private_tags() {
        let input = "A <private>secret</private> B <memory-global>mem</memory-global> C";
        assert_eq!(filter_injected_memory(input), "A <private>secret</private> B  C");
    }

    // ========================================================================
    // Regression tests: adversarial attack vectors against filter_injected_memory
    // ========================================================================

    // --- VULNERABILITY: Unclosed tags bypass filter entirely ---
    // An IDE plugin crash or malicious input can produce unclosed memory tags.
    // The regex requires a closing tag, so unclosed content leaks through unfiltered.
    #[test]
    fn filter_memory_unclosed_tag_stripped() {
        let input = "before <memory-global>leaked secret content";
        let result = filter_injected_memory(input);
        // Unclosed tags stripped to end-of-string (truncation safety)
        assert_eq!(result, "before ");
    }

    // --- VULNERABILITY: Tags with attributes bypass filter ---
    // If the IDE plugin or a future version adds attributes (class, id, data-*),
    // the regex fails to match because it expects `>` right after `[a-z]+`.
    #[test]
    fn filter_memory_tag_with_attributes_stripped() {
        let input = r#"before <memory-global class="injected">secret</memory-global> after"#;
        let result = filter_injected_memory(input);
        // [^>]* in regex allows attributes before >
        assert_eq!(result, "before  after");
    }

    #[test]
    fn filter_memory_tag_with_data_attribute_stripped() {
        let input = r#"<memory-project data-source="plugin">observations</memory-project> tail"#;
        let result = filter_injected_memory(input);
        assert_eq!(result, " tail");
    }

    // --- VULNERABILITY: Hyphenated suffixes bypass filter ---
    // `[a-z]+` stops at the first non-alpha character. Tags like <memory-global-v2>
    // or <memory-per-file> have hyphens in the suffix that break the match.
    #[test]
    fn filter_memory_hyphenated_suffix_matched() {
        let input = "<memory-global-v2>secret data</memory-global-v2> after";
        let result = filter_injected_memory(input);
        // [^>]* absorbs -v2, matching the full tag
        assert_eq!(result, " after");
    }

    #[test]
    fn filter_memory_multi_hyphen_suffix_matched() {
        let input = "<memory-per-file-cache>data</memory-per-file-cache>";
        let result = filter_injected_memory(input);
        // After regex fix: [a-z][-a-z]* matches multi-hyphen suffixes
        assert_eq!(result, "");
    }

    // --- VULNERABILITY: Nested tags — inner stripped, outer tags leak ---
    // When memory tags are nested, the lazy `.*?` matches the inner pair,
    // leaving the outer opening and closing tags as orphaned text in output.
    #[test]
    fn filter_memory_nested_tags_partial_strip() {
        let input = "<memory-global><memory-project>inner secret</memory-project></memory-global>";
        let result = filter_injected_memory(input);
        // Lazy .*? matches inner pair, leaves orphaned </memory-global>
        // Unclosed regex doesn't match </... (no opening tag pattern)
        assert_eq!(result, "</memory-global>");
    }

    #[test]
    fn filter_memory_nested_different_types() {
        let input = "head <memory-global>outer <memory-session>inner</memory-session> tail</memory-global> end";
        let result = filter_injected_memory(input);
        // Lazy .*? matches from <memory-global> to first </memory-*>
        // Leaves orphaned " tail</memory-global>"
        // Acceptable: real IDE never nests memory tags
        assert_eq!(result, "head  tail</memory-global> end");
    }

    // --- VULNERABILITY: Mismatched tag names still match ---
    // The regex doesn't enforce that open and close tag suffixes are identical.
    // <memory-foo>...</memory-bar> matches — could strip legitimate content.
    #[test]
    fn filter_memory_mismatched_tags_match() {
        let input = "<memory-foo>content</memory-bar>";
        let result = filter_injected_memory(input);
        // This matches because open/close suffixes are independent [a-z]+ patterns.
        // Potentially dangerous: could strip content between unrelated tags.
        assert_eq!(result, "");
        // Note: this is a permissiveness issue, not a bypass. But it means
        // crafted input can cause unexpected stripping of non-memory content.
    }

    // --- VULNERABILITY: Numeric/alphanumeric suffixes bypass ---
    // `[a-z]+` doesn't match digits. Tags like <memory-v2> bypass.
    #[test]
    fn filter_memory_numeric_suffix_matched() {
        let input = "<memory-v2>secret</memory-v2> rest";
        let result = filter_injected_memory(input);
        // [^>]* absorbs "v2"
        assert_eq!(result, " rest");
    }

    // --- VULNERABILITY: Whitespace inside tag bypasses ---
    #[test]
    fn filter_memory_whitespace_in_tag_stripped() {
        let input = "<memory-global >content</memory-global> after";
        let result = filter_injected_memory(input);
        // [^>]* absorbs space before >
        assert_eq!(result, " after");
    }

    #[test]
    fn filter_memory_newline_in_tag_stripped() {
        let input = "<memory-global\n>content</memory-global> after";
        let result = filter_injected_memory(input);
        // (?s) makes [^>]* match newline before >
        assert_eq!(result, " after");
    }

    // --- FALSE POSITIVE: Code discussions about memory tags get stripped ---
    #[test]
    fn filter_memory_code_discussion_false_positive() {
        let input = "The IDE uses <memory-global>...</memory-global> tags for injection.";
        let result = filter_injected_memory(input);
        // Legitimate discussion about the tag format gets stripped.
        // This is a false positive — the user was talking ABOUT the tags.
        assert_eq!(result, "The IDE uses  tags for injection.");
        // Note: This is inherently hard to fix without context awareness,
        // but it should be documented as a known false positive.
    }

    #[test]
    fn filter_memory_markdown_code_block_false_positive() {
        let input = "Example:\n```\n<memory-global>example data</memory-global>\n```\nEnd";
        let result = filter_injected_memory(input);
        // Content inside markdown code blocks gets stripped — false positive.
        // The regex has no awareness of code block boundaries.
        assert_eq!(result, "Example:\n```\n\n```\nEnd");
        // DESIRED: preserve content inside code blocks
        // assert_eq!(result, input);
    }

    // --- SAFETY: ReDoS resistance (Rust `regex` crate = guaranteed O(n)) ---
    #[test]
    fn filter_memory_large_content_no_redos() {
        // 1MB of content between tags — should complete in bounded time.
        let big_content = "x".repeat(1_000_000);
        let input = format!("<memory-global>{}</memory-global>", big_content);
        let start = std::time::Instant::now();
        let result = filter_injected_memory(&input);
        let elapsed = start.elapsed();
        assert_eq!(result, "");
        // Rust regex crate guarantees O(n) — this should be fast.
        assert!(elapsed.as_secs() < 2, "Regex took {:?} — potential ReDoS", elapsed);
    }

    #[test]
    fn filter_memory_large_content_no_match_no_redos() {
        // 1MB of content with unclosed tag — now stripped by unclosed regex
        let big_content = "x".repeat(1_000_000);
        let input = format!("<memory-global>{}", big_content);
        let start = std::time::Instant::now();
        let result = filter_injected_memory(&input);
        let elapsed = start.elapsed();
        assert_eq!(result, "");
        assert!(
            elapsed.as_secs() < 2,
            "Regex took {:?} on unclosed tag — potential ReDoS",
            elapsed
        );
    }

    // --- SAFETY: Greedy matching across blocks (should be lazy) ---
    #[test]
    fn filter_memory_lazy_match_does_not_cross_blocks() {
        let input = "<memory-global>a</memory-global> KEEP THIS <memory-project>b</memory-project>";
        let result = filter_injected_memory(input);
        // Lazy .*? should match each block independently, preserving text between.
        assert_eq!(result, " KEEP THIS ");
    }

    // --- EDGE: Self-closing/empty tag variant ---
    #[test]
    fn filter_memory_empty_tag() {
        let input = "before <memory-global></memory-global> after";
        let result = filter_injected_memory(input);
        assert_eq!(result, "before  after");
    }

    // --- EDGE: Mixed case suffix ---
    #[test]
    fn filter_memory_mixed_case_suffix() {
        let input = "<Memory-Global>content</Memory-Global> rest";
        let result = filter_injected_memory(input);
        // (?i) flag makes this case-insensitive — should strip
        assert_eq!(result, " rest");
    }

    // --- VULNERABILITY: Tag name with underscore bypasses ---
    #[test]
    fn filter_memory_underscore_suffix_matched() {
        let input = "<memory-per_project>data</memory-per_project> rest";
        let result = filter_injected_memory(input);
        // [^>]* absorbs underscore
        assert_eq!(result, " rest");
    }

    // --- EDGE: Multiple unclosed tags accumulate ---
    #[test]
    fn filter_memory_multiple_unclosed_tags_stripped() {
        let input = "<memory-global>leak1 <memory-project>leak2 <memory-session>leak3";
        let result = filter_injected_memory(input);
        // First unclosed regex matches from <memory-global> to end
        assert_eq!(result, "");
    }

    // --- EDGE: Closing tag without opening tag ---
    #[test]
    fn filter_memory_orphaned_close_tag() {
        let input = "before </memory-global> after";
        let result = filter_injected_memory(input);
        // Orphaned close tag passes through (no open to pair with)
        assert_eq!(result, "before </memory-global> after");
    }
}
