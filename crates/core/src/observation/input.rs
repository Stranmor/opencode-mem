//! Input types for observation creation from tool calls.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{NoiseLevel, ObservationType};
use crate::identifiers::{ObservationId, SessionId};
use crate::session::{SessionSummary, UserPrompt};

/// Trait for types that carry a relevance score (NaN-safe descending sort).
pub trait Scored {
    fn score(&self) -> f64;
}

/// Sort a slice of [`Scored`] items by score descending (NaN-safe).
///
/// NaN values are placed at the end of the list.
pub fn sort_by_score_descending<T: Scored>(items: &mut [T]) {
    items.sort_by(|a, b| {
        let sa = a.score();
        let sb = b.score();
        if sa.is_nan() && sb.is_nan() {
            std::cmp::Ordering::Equal
        } else if sa.is_nan() {
            std::cmp::Ordering::Greater
        } else if sb.is_nan() {
            std::cmp::Ordering::Less
        } else {
            sb.total_cmp(&sa)
        }
    });
}

impl Scored for SearchResult {
    fn score(&self) -> f64 {
        self.score
    }
}

/// Blanket impl for `(T, f64)` tuples where the second element is the score.
impl<T> Scored for (T, f64) {
    fn score(&self) -> f64 {
        self.1
    }
}

/// Input for creating a new observation (from tool call)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolCall {
    pub tool: String,
    pub session_id: SessionId,
    pub call_id: String,
    pub project: Option<String>,
    pub input: serde_json::Value,
    pub output: String,
}

impl ToolCall {
    #[must_use]
    pub fn new(
        tool: String,
        session_id: SessionId,
        call_id: String,
        project: Option<String>,
        input: serde_json::Value,
        output: String,
    ) -> Self {
        Self {
            tool,
            session_id,
            call_id,
            project,
            input,
            output,
        }
    }

    #[must_use]
    pub fn with_session_id(self, session_id: SessionId) -> Self {
        Self { session_id, ..self }
    }
}

/// Input for creating a new observation (compressed version)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ObservationInput {
    pub tool: String,
    pub session_id: SessionId,
    pub call_id: String,
    pub output: ToolOutput,
}

impl ObservationInput {
    #[must_use]
    pub fn new(tool: String, session_id: SessionId, call_id: String, output: ToolOutput) -> Self {
        Self {
            tool,
            session_id,
            call_id,
            output,
        }
    }
}

/// Output from a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ToolOutput {
    pub title: String,
    pub output: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl ToolOutput {
    #[must_use]
    pub fn new(title: String, output: String, metadata: serde_json::Value) -> Self {
        Self {
            title,
            output,
            metadata,
        }
    }
}

/// Ranked item from unified search across observations, sessions, and prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RankedItem {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub collection: String,
    pub score: f64,
}

impl Scored for RankedItem {
    fn score(&self) -> f64 {
        self.score
    }
}

/// Combined search result across observations, sessions, and prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UnifiedSearchResult {
    pub observations: Vec<SearchResult>,
    pub sessions: Vec<SessionSummary>,
    pub prompts: Vec<UserPrompt>,
    pub ranked: Vec<RankedItem>,
}

/// Compact observation for search results (index layer)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ObservationIndex {
    pub id: ObservationId,
    pub title: String,
    pub subtitle: Option<String>,
    pub observation_type: ObservationType,
    #[serde(default)]
    pub noise_level: NoiseLevel,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SearchResult {
    pub id: ObservationId,
    /// Observation title
    pub title: String,
    pub subtitle: Option<String>,
    pub observation_type: ObservationType,
    #[serde(default)]
    pub noise_level: NoiseLevel,
    pub score: f64,
}

impl SearchResult {
    #[must_use]
    pub fn new(
        id: ObservationId,
        title: String,
        subtitle: Option<String>,
        observation_type: ObservationType,
        noise_level: NoiseLevel,
        score: f64,
    ) -> Self {
        Self {
            id,
            title,
            subtitle,
            observation_type,
            noise_level,
            score,
        }
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
