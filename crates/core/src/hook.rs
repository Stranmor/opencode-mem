//! Hook event types for IDE integration
//!
//! Hooks are triggered by IDE events and call HTTP endpoints on the worker service.

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Hook event types triggered by IDE/CLI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
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
            Self::Context => return write!(f, "context"),
            Self::SessionInit => return write!(f, "session-init"),
            Self::Observation => return write!(f, "observation"),
            Self::Summarize => return write!(f, "summarize"),
        }
    }
}

impl FromStr for HookEvent {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "context" => return Ok(Self::Context),
            "session-init" | "session_init" => return Ok(Self::SessionInit),
            "observation" | "observe" => return Ok(Self::Observation),
            "summarize" => return Ok(Self::Summarize),
            _ => return Err(anyhow::anyhow!("Invalid hook event: {s}")),
        }
    }
}

/// Request payload for context hook
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ContextHookRequest {
    /// Project path or name to get context for.
    pub project: String,
    /// Maximum number of observations to return.
    #[serde(default = "default_context_limit")]
    pub limit: usize,
}

/// Default limit for context hook requests.
const fn default_context_limit() -> usize {
    return 50;
}

/// Request payload for session-init hook
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SessionInitHookRequest {
    /// Content session ID from the IDE.
    #[serde(rename = "contentSessionId")]
    pub content_session_id: String,
    /// Project path or name.
    pub project: Option<String>,
    /// Initial user prompt text.
    #[serde(rename = "userPrompt")]
    pub user_prompt: Option<String>,
}

impl SessionInitHookRequest {
    /// Creates a new session init hook request.
    #[must_use]
    pub const fn new(
        content_session_id: String,
        project: Option<String>,
        user_prompt: Option<String>,
    ) -> Self {
        return Self { content_session_id, project, user_prompt };
    }
}

/// Request payload for observation hook
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ObservationHookRequest {
    /// Tool name that was executed.
    pub tool: String,
    /// Session ID this observation belongs to.
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    /// Unique call ID for this tool invocation.
    #[serde(rename = "callId")]
    pub call_id: Option<String>,
    /// Project path or name.
    pub project: Option<String>,
    /// Tool input parameters.
    pub input: Option<serde_json::Value>,
    /// Tool output text.
    pub output: String,
}

impl ObservationHookRequest {
    /// Creates a new observation hook request.
    #[must_use]
    pub const fn new(
        tool: String,
        session_id: Option<String>,
        call_id: Option<String>,
        project: Option<String>,
        input: Option<serde_json::Value>,
        output: String,
    ) -> Self {
        return Self { tool, session_id, call_id, project, input, output };
    }
}

/// Request payload for summarize hook
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SummarizeHookRequest {
    /// Content session ID from the IDE.
    #[serde(rename = "contentSessionId")]
    pub content_session_id: Option<String>,
    /// Memory session ID.
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
}

impl SummarizeHookRequest {
    /// Creates a new summarize hook request.
    #[must_use]
    pub const fn new(content_session_id: Option<String>, session_id: Option<String>) -> Self {
        return Self { content_session_id, session_id };
    }
}
