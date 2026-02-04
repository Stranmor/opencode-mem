//! Global Knowledge Layer types
//!
//! Cross-project knowledge that allows AI agents to learn skills/patterns once
//! and apply them across ALL projects (solving the "Groundhog Day" problem).

use serde::{Deserialize, Serialize};

/// Type of knowledge entry
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
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
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Pattern => "pattern",
            Self::Gotcha => "gotcha",
            Self::Architecture => "architecture",
            Self::ToolUsage => "tool_usage",
        }
    }
}

impl std::str::FromStr for KnowledgeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "skill" => Ok(Self::Skill),
            "pattern" => Ok(Self::Pattern),
            "gotcha" => Ok(Self::Gotcha),
            "architecture" => Ok(Self::Architecture),
            "tool_usage" | "toolusage" => Ok(Self::ToolUsage),
            other => Err(format!("unknown knowledge type: {}", other)),
        }
    }
}

/// Global knowledge entry that applies across projects
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Input for creating new knowledge
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Search result with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchResult {
    /// The knowledge entry
    pub knowledge: GlobalKnowledge,
    /// Relevance score from search
    pub relevance_score: f64,
}

/// LLM extraction result for knowledge promotion
#[derive(Debug, Clone, Serialize, Deserialize)]
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
