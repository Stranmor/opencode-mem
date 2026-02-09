//! Observation types for coding session capture.

mod builder;
mod input;

pub use builder::*;
pub use input::*;

use std::str::FromStr;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Regex pattern for matching private content tags.
#[expect(clippy::unwrap_used, reason = "static regex pattern is compile-time validated")]
static PRIVATE_TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(?is)<private>.*?</private>").unwrap());

/// Filters out content wrapped in `<private>...</private>` tags.
pub fn filter_private_content(text: &str) -> String {
    PRIVATE_TAG_REGEX.replace_all(text, "").into_owned()
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
    type Err = String;

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
            other => Err(format!("unknown observation type: {other}")),
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
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "critical" => Ok(Self::Critical),
            "high" => Ok(Self::High),
            "medium" => Ok(Self::Medium),
            "low" => Ok(Self::Low),
            "negligible" => Ok(Self::Negligible),
            other => Err(format!("unknown noise level: {other}")),
        }
    }
}

/// Pre-filter for low-value observations that should not be persisted.
/// Checked at ALL entry points (SPOT — Single Point of Truth).
pub fn is_low_value_observation(title: &str) -> bool {
    let t = title.to_lowercase();

    if t.contains("file edit applied successfully")
        || t.contains("edit applied")
        || t.contains("successful file edit")
    {
        return true;
    }

    if t.contains("rustfmt") && t.contains("nightly") {
        return true;
    }

    if t.contains("task completion signal") {
        return true;
    }

    if (t.contains("comment") || t.contains("docstring")) && t.contains("hook") {
        return true;
    }

    if t.contains("memory classification") {
        return true;
    }

    if t.contains("tool call observed") || t.contains("tool execution") {
        return true;
    }

    if t.contains("no significant") {
        return true;
    }

    // AGENTS.md paraphrases — agent config/behavioral rule observations
    if t.starts_with("agent ")
        && (t.contains("rules")
            || t.contains("protocol")
            || t.contains("guidelines")
            || t.contains("doctrine")
            || t.contains("principles")
            || t.contains("behavioral")
            || t.contains("operational")
            || t.contains("workflow")
            || t.contains("persona"))
    {
        return true;
    }

    // TODO/plan status updates — process noise, not knowledge
    if t.starts_with("updated todo")
        || t.starts_with("updated plan")
        || t.starts_with("updated task status")
        || t.starts_with("updated agents.md")
        || t == "task completion"
    {
        return true;
    }

    // Noise level/classification meta-observations
    if t.contains("noise level classification") || t.contains("memory storage classification") {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_value_edit_applied() {
        assert!(is_low_value_observation("File edit applied successfully"));
        assert!(is_low_value_observation("Edit Applied to config.rs"));
    }

    #[test]
    fn low_value_rustfmt_nightly() {
        assert!(is_low_value_observation("rustfmt nightly formatting"));
    }

    #[test]
    fn low_value_agent_rules() {
        assert!(is_low_value_observation("Agent behavioral protocol update"));
    }

    #[test]
    fn low_value_todo_updates() {
        assert!(is_low_value_observation("Updated TODO list"));
        assert!(is_low_value_observation("updated plan for deployment"));
    }

    #[test]
    fn high_value_passes_filter() {
        assert!(!is_low_value_observation("Database migration v10 added session_summaries"));
        assert!(!is_low_value_observation("Fixed race condition in queue processor"));
    }

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
}
