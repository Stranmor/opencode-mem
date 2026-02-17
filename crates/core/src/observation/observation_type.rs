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
