//! Session types for memory sessions.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::{DiscoveryTokens, PromptNumber};

/// A memory session tracking a coding activity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Session {
    /// Unique session identifier
    pub id: String,
    /// Content session ID from IDE
    pub content_session_id: String,
    /// Memory session ID
    pub memory_session_id: Option<String>,
    /// Project name or path
    pub project: String,
    /// Initial user prompt
    pub user_prompt: Option<String>,
    /// Session start time
    pub started_at: DateTime<Utc>,
    /// Session end time
    pub ended_at: Option<DateTime<Utc>>,
    /// Current session status
    pub status: SessionStatus,
    /// Prompt counter for this session
    pub prompt_counter: u32,
}

impl Session {
    /// Creates a new session.
    #[must_use]
    #[expect(clippy::too_many_arguments, reason = "session has many fields")]
    pub fn new(
        id: String,
        content_session_id: String,
        memory_session_id: Option<String>,
        project: String,
        user_prompt: Option<String>,
        started_at: DateTime<Utc>,
        ended_at: Option<DateTime<Utc>>,
        status: SessionStatus,
        prompt_counter: u32,
    ) -> Self {
        Self {
            id,
            content_session_id,
            memory_session_id,
            project,
            user_prompt,
            started_at,
            ended_at,
            status,
            prompt_counter,
        }
    }
}

/// Session status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum SessionStatus {
    /// Session is active
    Active,
    /// Session completed successfully
    Completed,
    /// Session failed
    Failed,
}

impl SessionStatus {
    /// Returns the string representation of the session status.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match *self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

impl FromStr for SessionStatus {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            _ => Err(CoreError::InvalidSessionStatus(s.to_owned())),
        }
    }
}

/// Summary of a completed session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SessionSummary {
    /// Session ID this summary belongs to
    pub session_id: String,
    /// Project name
    pub project: String,
    /// What was requested
    pub request: Option<String>,
    /// What was investigated
    pub investigated: Option<String>,
    /// What was learned
    pub learned: Option<String>,
    /// What was completed
    pub completed: Option<String>,
    /// Next steps
    pub next_steps: Option<String>,
    /// Additional notes
    pub notes: Option<String>,
    /// Files that were read
    pub files_read: Vec<String>,
    /// Files that were edited
    pub files_edited: Vec<String>,
    /// Prompt number
    pub prompt_number: Option<PromptNumber>,
    /// Discovery tokens used
    pub discovery_tokens: Option<DiscoveryTokens>,
    /// When summary was created
    pub created_at: DateTime<Utc>,
}

impl SessionSummary {
    /// Creates a new session summary.
    #[must_use]
    #[expect(clippy::too_many_arguments, reason = "summary has many fields")]
    pub fn new(
        session_id: String,
        project: String,
        request: Option<String>,
        investigated: Option<String>,
        learned: Option<String>,
        completed: Option<String>,
        next_steps: Option<String>,
        notes: Option<String>,
        files_read: Vec<String>,
        files_edited: Vec<String>,
        prompt_number: Option<PromptNumber>,
        discovery_tokens: Option<DiscoveryTokens>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            session_id,
            project,
            request,
            investigated,
            learned,
            completed,
            next_steps,
            notes,
            files_read,
            files_edited,
            prompt_number,
            discovery_tokens,
            created_at,
        }
    }
}

/// User prompt within a session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct UserPrompt {
    /// Unique prompt ID
    pub id: String,
    /// Content session ID
    pub content_session_id: String,
    /// Prompt number in session
    pub prompt_number: PromptNumber,
    /// Prompt text content
    pub prompt_text: String,
    /// Project context
    pub project: Option<String>,
    /// When prompt was created
    pub created_at: DateTime<Utc>,
}

impl UserPrompt {
    /// Creates a new user prompt.
    #[must_use]
    pub fn new(
        id: String,
        content_session_id: String,
        prompt_number: PromptNumber,
        prompt_text: String,
        project: Option<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self { id, content_session_id, prompt_number, prompt_text, project, created_at }
    }
}
