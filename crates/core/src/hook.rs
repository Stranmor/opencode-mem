//! Hook event types for IDE integration
//!
//! Hooks are triggered by IDE events and call HTTP endpoints on the worker service.

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Hook event types triggered by IDE/CLI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    /// Context injection - get relevant observations for current project
    Context,
    /// Session initialization - start a new memory session
    SessionInit,
    /// Observation - record a tool call/output
    Observation,
    /// Summarize - generate session summary
    Summarize,
}

impl Display for HookEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match *self {
            Self::Context => write!(f, "context"),
            Self::SessionInit => write!(f, "session-init"),
            Self::Observation => write!(f, "observation"),
            Self::Summarize => write!(f, "summarize"),
        }
    }
}

impl FromStr for HookEvent {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "context" => Ok(Self::Context),
            "session-init" | "session_init" => Ok(Self::SessionInit),
            "observation" | "observe" => Ok(Self::Observation),
            "summarize" => Ok(Self::Summarize),
            _ => Err(anyhow::anyhow!("Invalid hook event: {s}")),
        }
    }
}

/// Request payload for context hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextHookRequest {
    pub project: String,
    #[serde(default = "default_context_limit")]
    pub limit: usize,
}

const fn default_context_limit() -> usize {
    50
}

/// Request payload for session-init hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInitHookRequest {
    #[serde(rename = "contentSessionId")]
    pub content_session_id: String,
    pub project: Option<String>,
    #[serde(rename = "userPrompt")]
    pub user_prompt: Option<String>,
}

/// Request payload for observation hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationHookRequest {
    pub tool: String,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    #[serde(rename = "callId")]
    pub call_id: Option<String>,
    pub project: Option<String>,
    pub input: Option<serde_json::Value>,
    pub output: String,
}

/// Request payload for summarize hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummarizeHookRequest {
    #[serde(rename = "contentSessionId")]
    pub content_session_id: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
}
