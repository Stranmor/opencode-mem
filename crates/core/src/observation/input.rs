//! Input types for observation creation from tool calls.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{NoiseLevel, ObservationType};

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
    pub fn new(tool: String, session_id: String, call_id: String, output: ToolOutput) -> Self {
        Self { tool, session_id, call_id, output }
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
    pub fn new(title: String, output: String, metadata: serde_json::Value) -> Self {
        Self { title, output, metadata }
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
    pub fn new(
        id: String,
        title: String,
        subtitle: Option<String>,
        observation_type: ObservationType,
        noise_level: NoiseLevel,
        score: f64,
    ) -> Self {
        Self { id, title, subtitle, observation_type, noise_level, score }
    }

    /// Converts a full Observation into a compact SearchResult with default score.
    #[must_use]
    pub fn from_observation(obs: &crate::Observation) -> Self {
        Self {
            id: obs.id.clone(),
            title: obs.title.clone(),
            subtitle: obs.subtitle.clone(),
            observation_type: obs.observation_type,
            noise_level: obs.noise_level,
            score: 0.0,
        }
    }
}
