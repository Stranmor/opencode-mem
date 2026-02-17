//! Observation types for coding session capture.

mod builder;
mod input;
mod low_value_filter;

mod dedup;
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
#[expect(clippy::unwrap_used, reason = "static regex pattern is compile-time validated")]
static MEMORY_TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<memory-[a-z]+>.*?</memory-[a-z]+>").unwrap());

/// Strips injected memory blocks (`<memory-*>...</memory-*>`) from text.
///
/// Memory blocks are injected into conversation context by the IDE plugin.
/// Without filtering, the observe hook re-processes them, creating duplicate
/// observations that get re-injected — causing infinite recursion.
pub fn filter_injected_memory(text: &str) -> String {
    MEMORY_TAG_REGEX.replace_all(text, "").into_owned()
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
}
