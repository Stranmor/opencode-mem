//! Global Knowledge Layer types
//!
//! Cross-project knowledge that allows AI agents to learn skills/patterns once
//! and apply them across ALL projects (solving the "Groundhog Day" problem).

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Type of knowledge entry
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum KnowledgeType {
    /// How to do something (tokio setup, error handling patterns)
    Skill,
    /// Reusable code/architecture pattern
    Pattern,
    /// Common pitfall to avoid
    Gotcha,
    /// System design decision
    Architecture,
    /// How to use external tool/library
    ToolUsage,
}

impl KnowledgeType {
    /// Returns the string representation of this knowledge type.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        return match *self {
            Self::Skill => "skill",
            Self::Pattern => "pattern",
            Self::Gotcha => "gotcha",
            Self::Architecture => "architecture",
            Self::ToolUsage => "tool_usage",
        };
    }
}

impl FromStr for KnowledgeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        return match s.to_lowercase().as_str() {
            "skill" => Ok(Self::Skill),
            "pattern" => Ok(Self::Pattern),
            "gotcha" => Ok(Self::Gotcha),
            "architecture" => Ok(Self::Architecture),
            "tool_usage" | "toolusage" => Ok(Self::ToolUsage),
            other => Err(format!("unknown knowledge type: {other}")),
        };
    }
}

/// Global knowledge entry that applies across projects
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GlobalKnowledge {
    /// Unique identifier
    pub id: String,
    /// Type of knowledge
    pub knowledge_type: KnowledgeType,
    /// Concise title
    pub title: String,
    /// Detailed description
    pub description: String,
    /// For skills: step-by-step how to apply
    pub instructions: Option<String>,
    /// Keywords/contexts when to use this knowledge
    pub triggers: Vec<String>,
    /// Projects where this was learned
    pub source_projects: Vec<String>,
    /// Observation IDs that contributed to this knowledge
    pub source_observations: Vec<String>,
    /// Confidence score 0.0-1.0, increases with usage/confirmation
    pub confidence: f64,
    /// Number of times this knowledge was used
    pub usage_count: i64,
    /// When this knowledge was last used
    pub last_used_at: Option<String>,
    /// When this knowledge was created
    pub created_at: String,
    /// When this knowledge was last updated
    pub updated_at: String,
}

impl GlobalKnowledge {
    /// Creates a new global knowledge entry.
    #[must_use]
    #[expect(clippy::too_many_arguments, reason = "knowledge has many fields")]
    pub const fn new(
        id: String,
        knowledge_type: KnowledgeType,
        title: String,
        description: String,
        instructions: Option<String>,
        triggers: Vec<String>,
        source_projects: Vec<String>,
        source_observations: Vec<String>,
        confidence: f64,
        usage_count: i64,
        last_used_at: Option<String>,
        created_at: String,
        updated_at: String,
    ) -> Self {
        return Self {
            id,
            knowledge_type,
            title,
            description,
            instructions,
            triggers,
            source_projects,
            source_observations,
            confidence,
            usage_count,
            last_used_at,
            created_at,
            updated_at,
        };
    }
}

/// Input for creating new knowledge
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct KnowledgeInput {
    /// Type of knowledge
    pub knowledge_type: KnowledgeType,
    /// Concise title
    pub title: String,
    /// Detailed description
    pub description: String,
    /// For skills: step-by-step how to apply
    pub instructions: Option<String>,
    /// Keywords/contexts when to use this knowledge
    pub triggers: Vec<String>,
    /// Source project (if any)
    pub source_project: Option<String>,
    /// Source observation ID (if any)
    pub source_observation: Option<String>,
}

impl KnowledgeInput {
    /// Creates a new knowledge input.
    #[must_use]
    pub const fn new(
        knowledge_type: KnowledgeType,
        title: String,
        description: String,
        instructions: Option<String>,
        triggers: Vec<String>,
        source_project: Option<String>,
        source_observation: Option<String>,
    ) -> Self {
        return Self {
            knowledge_type,
            title,
            description,
            instructions,
            triggers,
            source_project,
            source_observation,
        };
    }
}

/// Search result with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct KnowledgeSearchResult {
    /// The knowledge entry
    pub knowledge: GlobalKnowledge,
    /// Relevance score from search
    pub relevance_score: f64,
}

impl KnowledgeSearchResult {
    /// Creates a new knowledge search result.
    #[must_use]
    pub const fn new(knowledge: GlobalKnowledge, relevance_score: f64) -> Self {
        return Self { knowledge, relevance_score };
    }
}

/// LLM extraction result for knowledge promotion
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct KnowledgeExtractionResult {
    /// Whether to extract knowledge from this observation
    pub extract: bool,
    /// Reason for not extracting (if extract is false)
    pub reason: Option<String>,
    /// Knowledge type (if extract is true)
    pub knowledge_type: Option<String>,
    /// Title (if extract is true)
    pub title: Option<String>,
    /// Description (if extract is true)
    pub description: Option<String>,
    /// Instructions (if extract is true)
    pub instructions: Option<String>,
    /// Triggers (if extract is true)
    pub triggers: Option<Vec<String>>,
}
