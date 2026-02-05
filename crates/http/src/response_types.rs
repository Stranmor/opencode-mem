//! Response types (Serialize)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use opencode_mem_core::{SearchResult, SessionStatus, SessionSummary, UserPrompt};
use opencode_mem_storage::{PendingMessage, QueueStats};

#[derive(Debug, Serialize, Deserialize)]
pub struct ObserveResponse {
    pub id: String,
    pub queued: bool,
}

#[derive(Debug, Serialize)]
pub struct SessionInitResponse {
    pub session_id: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct SessionObservationsResponse {
    pub queued: usize,
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct SessionStatusResponse {
    pub session_id: String,
    pub status: SessionStatus,
    pub observation_count: usize,
    pub started_at: String,
    pub ended_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionDeleteResponse {
    pub deleted: bool,
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct SessionCompleteResponse {
    pub session_id: String,
    pub status: SessionStatus,
    pub summary: Option<String>,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct ReadinessResponse {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<&'static str>,
}

#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct VersionResponse {
    pub version: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub struct Settings {
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub mcp_enabled: bool,
    #[serde(default)]
    pub current_branch: String,
    #[serde(default)]
    pub log_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UnifiedSearchResult {
    pub observations: Vec<SearchResult>,
    pub sessions: Vec<SessionSummary>,
    pub prompts: Vec<UserPrompt>,
}

#[derive(Debug, Serialize)]
pub struct TimelineResult {
    pub anchor: Option<SearchResult>,
    pub before: Vec<SearchResult>,
    pub after: Vec<SearchResult>,
}

#[derive(Debug, Serialize)]
pub struct ContextPreview {
    pub project: String,
    pub observation_count: usize,
    pub preview: String,
}

#[derive(Debug, Serialize)]
pub struct SearchHelpResponse {
    pub endpoints: Vec<EndpointDoc>,
}

#[derive(Debug, Serialize)]
pub struct EndpointDoc {
    pub path: &'static str,
    pub method: &'static str,
    pub description: &'static str,
    pub params: Vec<ParamDoc>,
}

#[derive(Debug, Serialize)]
pub struct ParamDoc {
    pub name: &'static str,
    pub required: bool,
    pub description: &'static str,
}

#[derive(Debug, Serialize)]
pub struct PendingQueueResponse {
    pub messages: Vec<PendingMessage>,
    pub stats: QueueStats,
}

#[derive(Debug, Serialize)]
pub struct ProcessQueueResponse {
    pub processed: usize,
    pub failed: usize,
}

#[derive(Debug, Serialize)]
pub struct ClearQueueResponse {
    pub cleared: usize,
}

#[derive(Debug, Serialize)]
pub struct ProcessingStatusResponse {
    pub active: bool,
    pub pending_count: usize,
}

#[derive(Debug, Serialize)]
pub struct SetProcessingResponse {
    pub active: bool,
}

#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    pub settings: Settings,
}

#[derive(Debug, Serialize)]
pub struct McpStatusResponse {
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct BranchStatusResponse {
    pub current_branch: String,
    pub is_dirty: bool,
}

#[derive(Debug, Serialize)]
pub struct SwitchBranchResponse {
    pub success: bool,
    pub branch: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateBranchResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct InstructionsResponse {
    pub sections: Vec<String>,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct AdminResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct KnowledgeUsageResponse {
    pub success: bool,
    pub id: String,
}
