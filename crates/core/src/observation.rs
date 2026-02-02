use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bugfix" => Ok(Self::Bugfix),
            "feature" => Ok(Self::Feature),
            "refactor" => Ok(Self::Refactor),
            "discovery" => Ok(Self::Discovery),
            "decision" => Ok(Self::Decision),
            _ => Ok(Self::Change),
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
