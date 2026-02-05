//! Observation types for coding session capture.

use std::str::FromStr;
use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Regex pattern for matching private content tags.
#[expect(clippy::unwrap_used, reason = "static regex pattern is compile-time validated")]
static PRIVATE_TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| return Regex::new("(?is)<private>.*?</private>").unwrap());

/// Filters out content wrapped in `<private>...</private>` tags.
pub fn filter_private_content(text: &str) -> String {
    return PRIVATE_TAG_REGEX.replace_all(text, "").into_owned();
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
        return match *self {
            Self::Bugfix => "bugfix",
            Self::Feature => "feature",
            Self::Refactor => "refactor",
            Self::Change => "change",
            Self::Discovery => "discovery",
            Self::Decision => "decision",
            Self::Gotcha => "gotcha",
            Self::Preference => "preference",
        };
    }
}

impl FromStr for ObservationType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        return match s.to_lowercase().as_str() {
            "bugfix" => Ok(Self::Bugfix),
            "feature" => Ok(Self::Feature),
            "refactor" => Ok(Self::Refactor),
            "change" => Ok(Self::Change),
            "discovery" => Ok(Self::Discovery),
            "decision" => Ok(Self::Decision),
            "gotcha" => Ok(Self::Gotcha),
            "preference" => Ok(Self::Preference),
            other => Err(format!("unknown observation type: {other}")),
        };
    }
}

/// Structured observation of a coding activity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Observation {
    /// Unique identifier
    pub id: String,
    /// Session this observation belongs to
    pub session_id: String,
    /// Project this observation belongs to
    pub project: Option<String>,
    /// Type of observation
    pub observation_type: ObservationType,
    /// Concise title (max 100 chars)
    pub title: String,
    /// Optional one-line context
    pub subtitle: Option<String>,
    /// 2-3 sentence explanation of what happened and why
    pub narrative: Option<String>,
    /// Specific facts learned (file paths, function names, decisions)
    pub facts: Vec<String>,
    /// Semantic concepts for categorization
    pub concepts: Vec<Concept>,
    /// File paths mentioned or modified
    pub files_read: Vec<String>,
    /// File paths modified
    pub files_modified: Vec<String>,
    /// Semantic keywords for search
    pub keywords: Vec<String>,
    /// Prompt number within session
    pub prompt_number: Option<u32>,
    /// Token count for ROI tracking
    pub discovery_tokens: Option<u32>,
    /// Signal vs noise classification (Critical = must show, Negligible = hide by default)
    #[serde(default)]
    pub noise_level: NoiseLevel,
    /// Why this noise level was assigned
    pub noise_reason: Option<String>,
    /// When this observation was created
    pub created_at: DateTime<Utc>,
}

impl Observation {
    /// Creates a new observation.
    #[must_use]
    #[expect(clippy::too_many_arguments, reason = "observation has many fields")]
    pub const fn new(
        id: String,
        session_id: String,
        project: Option<String>,
        observation_type: ObservationType,
        title: String,
        subtitle: Option<String>,
        narrative: Option<String>,
        facts: Vec<String>,
        concepts: Vec<Concept>,
        files_read: Vec<String>,
        files_modified: Vec<String>,
        keywords: Vec<String>,
        prompt_number: Option<u32>,
        discovery_tokens: Option<u32>,
        noise_level: NoiseLevel,
        noise_reason: Option<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        return Self {
            id,
            session_id,
            project,
            observation_type,
            title,
            subtitle,
            narrative,
            facts,
            concepts,
            files_read,
            files_modified,
            keywords,
            prompt_number,
            discovery_tokens,
            noise_level,
            noise_reason,
            created_at,
        };
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
        return match *self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Negligible => "negligible",
        };
    }
}

impl FromStr for NoiseLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        return match s.to_lowercase().as_str() {
            "critical" => Ok(Self::Critical),
            "high" => Ok(Self::High),
            "medium" => Ok(Self::Medium),
            "low" => Ok(Self::Low),
            "negligible" => Ok(Self::Negligible),
            other => Err(format!("unknown noise level: {other}")),
        };
    }
}

/// Input for creating a new observation (from tool call)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolCall {
    /// Tool name that was called
    pub tool: String,
    /// Session ID for this tool call
    pub session_id: String,
    /// Unique call identifier
    pub call_id: String,
    /// Project context
    pub project: Option<String>,
    /// Tool input parameters
    pub input: serde_json::Value,
    /// Tool output result
    pub output: String,
}

impl ToolCall {
    /// Creates a new tool call.
    #[must_use]
    pub fn new(
        tool: String,
        session_id: String,
        call_id: String,
        project: Option<String>,
        input: serde_json::Value,
        output: String,
    ) -> Self {
        Self { tool, session_id, call_id, project, input, output }
    }

    /// Creates a new tool call with a different session ID.
    #[must_use]
    pub fn with_session_id(self, session_id: String) -> Self {
        Self { session_id, ..self }
    }
}

/// Input for creating a new observation (compressed version)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ObservationInput {
    /// Tool name
    pub tool: String,
    /// Session ID
    pub session_id: String,
    /// Call ID
    pub call_id: String,
    /// Tool output
    pub output: ToolOutput,
}

impl ObservationInput {
    /// Creates a new observation input.
    #[must_use]
    pub const fn new(
        tool: String,
        session_id: String,
        call_id: String,
        output: ToolOutput,
    ) -> Self {
        return Self { tool, session_id, call_id, output };
    }
}

/// Output from a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolOutput {
    /// Output title
    pub title: String,
    /// Output content
    pub output: String,
    /// Additional metadata
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl ToolOutput {
    /// Creates a new tool output.
    #[must_use]
    pub const fn new(title: String, output: String, metadata: serde_json::Value) -> Self {
        return Self { title, output, metadata };
    }
}

/// Compact observation for search results (index layer)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ObservationIndex {
    /// Observation ID
    pub id: String,
    /// Observation title
    pub title: String,
    /// Optional subtitle
    pub subtitle: Option<String>,
    /// Type of observation
    pub observation_type: ObservationType,
    /// Noise level classification
    #[serde(default)]
    pub noise_level: NoiseLevel,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

/// Search result with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SearchResult {
    /// Observation ID
    pub id: String,
    /// Observation title
    pub title: String,
    /// Optional subtitle
    pub subtitle: Option<String>,
    /// Type of observation
    pub observation_type: ObservationType,
    /// Noise level classification
    #[serde(default)]
    pub noise_level: NoiseLevel,
    /// Relevance score
    pub score: f64,
}

impl SearchResult {
    /// Creates a new search result.
    #[must_use]
    pub const fn new(
        id: String,
        title: String,
        subtitle: Option<String>,
        observation_type: ObservationType,
        noise_level: NoiseLevel,
        score: f64,
    ) -> Self {
        return Self { id, title, subtitle, observation_type, noise_level, score };
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
}
