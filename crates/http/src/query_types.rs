//! Request/query types (Deserialize)

use opencode_mem_core::{DEFAULT_QUERY_LIMIT, MAX_BATCH_IDS, MAX_QUERY_LIMIT};
use serde::Deserialize;
use std::collections::HashMap;

const fn default_limit() -> usize {
    DEFAULT_QUERY_LIMIT
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
    pub from: Option<String>,
    pub to: Option<String>,
}

impl SearchQuery {
    /// Cap limit to prevent DoS via unbounded queries.
    pub fn capped_limit(&self) -> usize {
        self.limit.min(MAX_QUERY_LIMIT)
    }
}

#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

impl TimelineQuery {
    pub fn capped_limit(&self) -> usize {
        self.limit.min(MAX_QUERY_LIMIT)
    }
}

#[derive(Debug, Deserialize)]
pub struct ContextQuery {
    pub project: String,
    #[serde(default = "default_context_limit")]
    pub limit: usize,
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub ids: Vec<String>,
}

impl BatchRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.ids.is_empty() {
            return Err("ids array must not be empty".to_owned());
        }
        if self.ids.len() > MAX_BATCH_IDS {
            return Err(format!("ids array exceeds maximum of {MAX_BATCH_IDS} items"));
        }
        Ok(())
    }
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

impl PaginationQuery {
    pub fn capped_limit(&self) -> usize {
        self.limit.min(MAX_QUERY_LIMIT)
    }
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

#[derive(Debug, Deserialize)]
pub struct SaveMemoryRequest {
    pub text: String,
    pub title: Option<String>,
    pub project: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_search_query_capped_limit() {
        let q: SearchQuery =
            serde_json::from_value(json!({"q": "x", "limit": 5000})).expect("valid SearchQuery");
        assert_eq!(q.capped_limit(), 1000);
    }

    #[test]
    fn test_search_query_normal_limit() {
        let q: SearchQuery =
            serde_json::from_value(json!({"q": "x", "limit": 50})).expect("valid SearchQuery");
        assert_eq!(q.capped_limit(), 50);
    }

    #[test]
    fn test_timeline_query_capped_limit() {
        let q: TimelineQuery =
            serde_json::from_value(json!({"limit": 5000})).expect("valid TimelineQuery");
        assert_eq!(q.capped_limit(), 1000);
    }

    #[test]
    fn test_pagination_query_capped_limit() {
        let q: PaginationQuery =
            serde_json::from_value(json!({"limit": 5000})).expect("valid PaginationQuery");
        assert_eq!(q.capped_limit(), 1000);
    }

    #[test]
    fn test_batch_request_validate_empty() {
        let req = BatchRequest { ids: vec![] };
        let err = req.validate().unwrap_err();
        assert!(err.contains("empty"), "Expected 'empty' in error: {err}");
    }

    #[test]
    fn test_batch_request_validate_too_many() {
        let ids: Vec<String> = (0..501).map(|i| format!("id-{i}")).collect();
        let req = BatchRequest { ids };
        let err = req.validate().unwrap_err();
        assert!(err.contains("500"), "Expected '500' in error: {err}");
    }

    #[test]
    fn test_batch_request_validate_ok() {
        let ids: Vec<String> = (0..10).map(|i| format!("id-{i}")).collect();
        let req = BatchRequest { ids };
        assert!(req.validate().is_ok());
    }
}
