use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

static PRIVATE_TAG_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<private>.*?</private>").expect("Invalid privacy tag regex")
});

/// Filters out content wrapped in `<private>...</private>` tags.
pub fn filter_private_content(text: &str) -> String {
    PRIVATE_TAG_REGEX.replace_all(text, "").into_owned()
}

/// Type of observation captured during a coding session
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ObservationType {
    /// Bug fix
    Bugfix,
    /// New feature implementation
    Feature,
    /// Code refactoring
    Refactor,
    /// General change
    Change,
    /// Discovery or learning
    Discovery,
    /// Architectural or design decision
    Decision,
}

impl ObservationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bugfix => "bugfix",
            Self::Feature => "feature",
            Self::Refactor => "refactor",
            Self::Change => "change",
            Self::Discovery => "discovery",
            Self::Decision => "decision",
        }
    }
}

impl std::str::FromStr for ObservationType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bugfix" => Ok(Self::Bugfix),
            "feature" => Ok(Self::Feature),
            "refactor" => Ok(Self::Refactor),
            "change" => Ok(Self::Change),
            "discovery" => Ok(Self::Discovery),
            "decision" => Ok(Self::Decision),
            other => Err(format!("unknown observation type: {}", other)),
        }
    }
}

/// Structured observation of a coding activity
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// When this observation was created
    pub created_at: DateTime<Utc>,
}

/// Semantic concepts for observation categorization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Concept {
    HowItWorks,
    WhyItExists,
    WhatChanged,
    ProblemSolution,
    Gotcha,
    Pattern,
    TradeOff,
}

/// Input for creating a new observation (from tool call)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    pub session_id: String,
    pub call_id: String,
    pub project: Option<String>,
    pub input: serde_json::Value,
    pub output: String,
}

/// Input for creating a new observation (compressed version)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationInput {
    pub tool: String,
    pub session_id: String,
    pub call_id: String,
    pub output: ToolOutput,
}

/// Output from a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub title: String,
    pub output: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Compact observation for search results (index layer)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationIndex {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub observation_type: ObservationType,
    pub created_at: DateTime<Utc>,
}

/// Search result with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub observation_type: ObservationType,
    pub score: f64,
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
