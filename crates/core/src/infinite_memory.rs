//! Domain types for the infinite memory system.
//!
//! These are the core data structures for raw events (never deleted),
//! hierarchical summaries (5min → hour → day), and structured entity extraction.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

/// Event types that can be stored in infinite memory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InfiniteEventType {
    User,
    Assistant,
    Tool,
    Decision,
    Error,
    Commit,
    Delegation,
}

impl InfiniteEventType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
            Self::Decision => "decision",
            Self::Error => "error",
            Self::Commit => "commit",
            Self::Delegation => "delegation",
        }
    }
}

impl fmt::Display for InfiniteEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for InfiniteEventType {
    type Err = InfiniteEventTypeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "tool" => Ok(Self::Tool),
            "decision" => Ok(Self::Decision),
            "error" => Ok(Self::Error),
            "commit" => Ok(Self::Commit),
            "delegation" => Ok(Self::Delegation),
            unknown => Err(InfiniteEventTypeParseError(unknown.to_owned())),
        }
    }
}

/// Error returned when parsing an unknown infinite event type string.
#[derive(Debug, Clone)]
pub struct InfiniteEventTypeParseError(pub String);

impl fmt::Display for InfiniteEventTypeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unknown infinite event type: '{}'", self.0)
    }
}

impl std::error::Error for InfiniteEventTypeParseError {}

/// Raw event to be stored in infinite memory (input, not yet persisted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawInfiniteEvent {
    pub session_id: String,
    pub project: Option<String>,
    pub event_type: InfiniteEventType,
    pub content: serde_json::Value,
    pub files: Vec<String>,
    pub tools: Vec<String>,
    pub call_id: Option<String>,
}

/// Stored event with database-assigned ID and timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredInfiniteEvent {
    pub id: i64,
    pub ts: DateTime<Utc>,
    pub session_id: String,
    pub project: Option<String>,
    pub event_type: InfiniteEventType,
    pub content: serde_json::Value,
    pub files: Vec<String>,
    pub tools: Vec<String>,
    pub call_id: Option<String>,
}

/// Structured entities extracted from summaries via LLM.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SummaryEntities {
    pub files: Vec<String>,
    pub functions: Vec<String>,
    pub libraries: Vec<String>,
    pub errors: Vec<String>,
    pub decisions: Vec<String>,
}

impl SummaryEntities {
    /// Merge multiple entities into one (for aggregation across time windows).
    #[must_use]
    pub fn merge(entities: &[Option<SummaryEntities>]) -> Option<SummaryEntities> {
        let mut files = HashSet::new();
        let mut functions = HashSet::new();
        let mut libraries = HashSet::new();
        let mut errors = HashSet::new();
        let mut decisions = HashSet::new();

        let mut has_any = false;
        for e in entities.iter().flatten() {
            has_any = true;
            files.extend(e.files.iter().cloned());
            functions.extend(e.functions.iter().cloned());
            libraries.extend(e.libraries.iter().cloned());
            errors.extend(e.errors.iter().cloned());
            decisions.extend(e.decisions.iter().cloned());
        }

        if !has_any {
            return None;
        }

        Some(SummaryEntities {
            files: files.into_iter().collect(),
            functions: functions.into_iter().collect(),
            libraries: libraries.into_iter().collect(),
            errors: errors.into_iter().collect(),
            decisions: decisions.into_iter().collect(),
        })
    }
}

/// Summary at various time scales (5min, hour, day).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfiniteSummary {
    pub id: i64,
    pub ts_start: DateTime<Utc>,
    pub ts_end: DateTime<Utc>,
    pub session_id: Option<String>,
    pub project: Option<String>,
    pub content: String,
    pub event_count: i32,
    pub entities: Option<SummaryEntities>,
}

/// Helper to create a tool event for infinite memory storage.
#[must_use]
pub fn tool_event(
    session_id: &str,
    project: Option<&str>,
    tool: &str,
    input: serde_json::Value,
    output: serde_json::Value,
    files: Vec<String>,
    call_id: Option<String>,
) -> RawInfiniteEvent {
    RawInfiniteEvent {
        session_id: session_id.to_string(),
        project: project.map(|s| s.to_string()),
        event_type: InfiniteEventType::Tool,
        content: serde_json::json!({
            "tool": tool,
            "input": input,
            "output": output
        }),
        files,
        tools: vec![tool.to_string()],
        call_id,
    }
}

/// Helper to create a user message event for infinite memory.
#[must_use]
pub fn user_event(session_id: &str, project: Option<&str>, message: &str) -> RawInfiniteEvent {
    RawInfiniteEvent {
        session_id: session_id.to_string(),
        project: project.map(|s| s.to_string()),
        event_type: InfiniteEventType::User,
        content: serde_json::json!({
            "text": message
        }),
        files: vec![],
        tools: vec![],
        call_id: None,
    }
}

/// Helper to create an assistant response event for infinite memory.
#[must_use]
pub fn assistant_event(
    session_id: &str,
    project: Option<&str>,
    response: &str,
    thinking: Option<&str>,
) -> RawInfiniteEvent {
    RawInfiniteEvent {
        session_id: session_id.to_string(),
        project: project.map(|s| s.to_string()),
        event_type: InfiniteEventType::Assistant,
        content: serde_json::json!({
            "text": response,
            "thinking": thinking
        }),
        files: vec![],
        tools: vec![],
        call_id: None,
    }
}
