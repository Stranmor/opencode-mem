//! Observation classification enums.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

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
    pub const ALL_VARIANTS_STR: &'static str =
        "bugfix|feature|refactor|change|discovery|decision|gotcha|preference";

    pub const ALL_VARIANTS: &'static [ObservationType] = &[
        ObservationType::Gotcha,
        ObservationType::Bugfix,
        ObservationType::Decision,
        ObservationType::Feature,
        ObservationType::Refactor,
        ObservationType::Change,
        ObservationType::Discovery,
        ObservationType::Preference,
    ];

    /// Returns the descriptive explanation for this observation type
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match *self {
            Self::Gotcha => "Something that broke, surprised you, or behaved unexpectedly.",
            Self::Bugfix => {
                "A bug was found AND fixed. What was wrong, why, and how it was solved."
            },
            Self::Decision => {
                "(critical only) An irreversible architectural choice with clear reasoning."
            },
            Self::Feature => "(critical only) A significant new capability was completed.",
            Self::Refactor => "Code structure was changed without altering external behavior.",
            Self::Change => "A general code change that is not a bugfix or a feature.",
            Self::Discovery => "Learning how existing code or an external API works.",
            Self::Preference => "User explicitly requested a specific way of doing things.",
        }
    }

    /// Returns formatting examples for this observation type
    #[must_use]
    pub const fn examples(&self) -> &'static [&'static str] {
        match *self {
            Self::Gotcha => &[
                "\"SQLite ALTER TABLE does not support adding STORED generated columns\"",
                "\"Claude thinking blocks cause Vertex AI API rejection\"",
            ],
            Self::Bugfix => {
                &["\"Advisory lock leak on connection drop — fixed with after_release hook\""]
            },
            Self::Decision => {
                &["\"Chose pgvector over ChromaDB for vector storage — no external dependency\""]
            },
            Self::Feature => {
                &["\"Implemented hybrid search: tsvector BM25 50% + vector cosine similarity 50%\""]
            },
            Self::Refactor => {
                &["\"Extracted memory filtering logic into core crate for reuse in CLI and MCP\""]
            },
            Self::Change => &["\"Updated Rust version to 1.76 and bumped dependencies\""],
            Self::Discovery => {
                &["\"GitHub search API limits results to 1000 items max regardless of pagination\""]
            },
            Self::Preference => {
                &["\"User prefers early returns over deeply nested if statements\""]
            },
        }
    }

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
    pub const ALL_VARIANTS_STR: &'static str =
        "how-it-works|why-it-exists|what-changed|problem-solution|gotcha|pattern|trade-off";

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

impl std::fmt::Display for Concept {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Concept {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "how-it-works" => Ok(Self::HowItWorks),
            "why-it-exists" => Ok(Self::WhyItExists),
            "what-changed" => Ok(Self::WhatChanged),
            "problem-solution" => Ok(Self::ProblemSolution),
            "gotcha" => Ok(Self::Gotcha),
            "pattern" => Ok(Self::Pattern),
            "trade-off" => Ok(Self::TradeOff),
            other => Err(CoreError::InvalidObservationType(format!("Unknown concept: {}", other))),
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
    pub const ALL_VARIANTS_STR: &'static str = "critical|high|medium|low|negligible";

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
