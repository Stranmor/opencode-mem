use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A coding session containing multiple observations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique identifier
    pub id: String,
    /// Content session ID (from OpenCode)
    pub content_session_id: String,
    /// Memory session ID (for SDK agent resume)
    pub memory_session_id: Option<String>,
    /// Project name
    pub project: String,
    /// Initial user prompt
    pub user_prompt: Option<String>,
    /// When session started
    pub started_at: DateTime<Utc>,
    /// When session ended
    pub ended_at: Option<DateTime<Utc>>,
    /// Session status
    pub status: SessionStatus,
    /// Number of prompts in session
    pub prompt_counter: u32,
}

/// Session status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Completed,
    Failed,
}

/// Structured session summary (generated at session end)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    /// Session this summary belongs to
    pub session_id: String,
    /// Project name
    pub project: String,
    /// What the user requested
    pub request: Option<String>,
    /// What was investigated
    pub investigated: Option<String>,
    /// What was learned
    pub learned: Option<String>,
    /// What was completed
    pub completed: Option<String>,
    /// Suggested next steps
    pub next_steps: Option<String>,
    /// Additional notes
    pub notes: Option<String>,
    /// Prompt number
    pub prompt_number: Option<u32>,
    /// Token count
    pub discovery_tokens: Option<u32>,
    /// When summary was created
    pub created_at: DateTime<Utc>,
}

/// User prompt record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPrompt {
    pub id: String,
    pub content_session_id: String,
    pub prompt_number: u32,
    pub prompt_text: String,
    pub project: Option<String>,
    pub created_at: DateTime<Utc>,
}
