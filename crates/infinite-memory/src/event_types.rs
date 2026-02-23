//! Event and summary types for Infinite AGI Memory.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

/// Event types that can be stored
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventType {
    User,
    Assistant,
    Tool,
    Decision,
    Error,
    Commit,
    Delegation,
}

impl EventType {
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

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EventType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "tool" => Ok(Self::Tool),
            "decision" => Ok(Self::Decision),
            "error" => Ok(Self::Error),
            "commit" => Ok(Self::Commit),
            "delegation" => Ok(Self::Delegation),
            unknown => {
                anyhow::bail!("Unknown event type: '{}'", unknown)
            },
        }
    }
}

/// Raw event to be stored
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawEvent {
    pub session_id: String,
    pub project: Option<String>,
    pub event_type: EventType,
    pub content: serde_json::Value,
    pub files: Vec<String>,
    pub tools: Vec<String>,
    pub call_id: Option<String>,
}

/// Stored event with ID and timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub id: i64,
    pub ts: DateTime<Utc>,
    pub session_id: String,
    pub project: Option<String>,
    pub event_type: EventType,
    pub content: serde_json::Value,
    pub files: Vec<String>,
    pub tools: Vec<String>,
    pub call_id: Option<String>,
}

/// Structured entities extracted from summaries
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SummaryEntities {
    pub files: Vec<String>,
    pub functions: Vec<String>,
    pub libraries: Vec<String>,
    pub errors: Vec<String>,
    pub decisions: Vec<String>,
}

impl SummaryEntities {
    /// Merge multiple entities into one (for aggregation)
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

/// Summary at various time scales
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub id: i64,
    pub ts_start: DateTime<Utc>,
    pub ts_end: DateTime<Utc>,
    pub session_id: Option<String>,
    pub project: Option<String>,
    pub content: String,
    pub event_count: i32,
    pub entities: Option<SummaryEntities>,
}

/// Helper to create tool event
pub fn tool_event(
    session_id: &str,
    project: Option<&str>,
    tool: &str,
    input: serde_json::Value,
    output: serde_json::Value,
    files: Vec<String>,
    call_id: Option<String>,
) -> RawEvent {
    RawEvent {
        session_id: session_id.to_string(),
        project: project.map(|s| s.to_string()),
        event_type: EventType::Tool,
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

/// Helper to create user message event
pub fn user_event(session_id: &str, project: Option<&str>, message: &str) -> RawEvent {
    RawEvent {
        session_id: session_id.to_string(),
        project: project.map(|s| s.to_string()),
        event_type: EventType::User,
        content: serde_json::json!({
            "text": message
        }),
        files: vec![],
        tools: vec![],
        call_id: None,
    }
}

/// Helper to create assistant response event
pub fn assistant_event(
    session_id: &str,
    project: Option<&str>,
    response: &str,
    thinking: Option<&str>,
) -> RawEvent {
    RawEvent {
        session_id: session_id.to_string(),
        project: project.map(|s| s.to_string()),
        event_type: EventType::Assistant,
        content: serde_json::json!({
            "text": response,
            "thinking": thinking
        }),
        files: vec![],
        tools: vec![],
        call_id: None,
    }
}
