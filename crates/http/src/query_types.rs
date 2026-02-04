//! Request/query types (Deserialize)

use serde::Deserialize;
use std::collections::HashMap;

const fn default_limit() -> usize {
    20
}

const fn default_context_limit() -> usize {
    50
}

const fn default_timeline_count() -> usize {
    5
}

fn default_preview_format() -> String {
    "compact".to_owned()
}

pub const fn default_infinite_limit() -> i64 {
    50
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub project: Option<String>,
    #[serde(rename = "type")]
    pub obs_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct ContextQuery {
    pub project: String,
    #[serde(default = "default_context_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SessionSummaryRequest {
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionInitRequest {
    #[serde(rename = "contentSessionId")]
    pub content_session_id: Option<String>,
    pub project: Option<String>,
    #[serde(rename = "userPrompt")]
    pub user_prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SessionObservationsRequest {
    #[serde(rename = "contentSessionId")]
    pub content_session_id: Option<String>,
    pub observations: Vec<opencode_mem_core::ToolCall>,
}

#[derive(Debug, Deserialize)]
pub struct SessionSummarizeRequest {
    #[serde(rename = "contentSessionId")]
    pub content_session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub project: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FileSearchQuery {
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct UnifiedTimelineQuery {
    pub anchor: Option<String>,
    pub q: Option<String>,
    #[serde(default = "default_timeline_count")]
    pub before: usize,
    #[serde(default = "default_timeline_count")]
    pub after: usize,
    #[expect(dead_code, reason = "Reserved for future project filtering")]
    pub project: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ContextPreviewQuery {
    pub project: String,
    #[serde(default = "default_context_limit")]
    pub limit: usize,
    #[serde(default = "default_preview_format")]
    pub format: String,
}

#[derive(Debug, Deserialize)]
pub struct SetProcessingRequest {
    pub active: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSettingsRequest {
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    #[expect(dead_code, reason = "Reserved for future log path configuration")]
    pub log_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ToggleMcpRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct SwitchBranchRequest {
    pub branch: String,
}

#[derive(Debug, Deserialize)]
pub struct InstructionsQuery {
    #[serde(default)]
    pub section: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InfiniteTimeRangeQuery {
    pub start: String,
    pub end: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchEntitiesQuery {
    pub entity_type: String,
    pub value: String,
    #[serde(default = "default_infinite_limit")]
    pub limit: i64,
}

#[derive(Debug, Deserialize)]
pub struct KnowledgeQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub knowledge_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SaveKnowledgeRequest {
    pub knowledge_type: String,
    pub title: String,
    pub description: String,
    pub instructions: Option<String>,
    #[serde(default)]
    pub triggers: Vec<String>,
    pub source_project: Option<String>,
    pub source_observation: Option<String>,
}
