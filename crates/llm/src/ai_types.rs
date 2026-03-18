use serde::{Deserialize, Deserializer, Serialize};

/// OpenAI-compatible `response_format.type` field.
/// Only `json_object` and `text` are valid values.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseFormatType {
    /// Structured JSON output mode
    JsonObject,
    /// Plain text output mode
    Text,
}

/// Deserializes a JSON `null` as `String::default()` (empty string).
///
/// Serde's `#[serde(default)]` only applies when the key is absent.
/// When the LLM returns `"type": null` (e.g. for negligible observations),
/// standard deserialization fails with "expected a string, found null".
fn null_as_default<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Option::<String>::deserialize(deserializer).map(|opt| opt.unwrap_or_default())
}

/// Deserializes a JSON `null`, wrong type, or absent key as empty `Vec<String>`.
///
/// LLM responses sometimes return `null`, objects, or other non-array types
/// for fields that should be string arrays. This gracefully handles all cases.
fn null_or_invalid_as_default_vec<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Array(arr) => Ok(arr
            .into_iter()
            .filter_map(|v| match v {
                serde_json::Value::String(s) => Some(s),
                _ => None,
            })
            .collect()),
        serde_json::Value::Null => Ok(Vec::new()),
        _ => Ok(Vec::new()),
    }
}

#[derive(Serialize, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub response_format: ResponseFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

#[derive(Serialize, Clone)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub format_type: ResponseFormatType,
}

#[derive(Serialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize)]
pub struct Choice {
    pub message: ResponseMessage,
}

#[derive(Deserialize)]
pub struct ResponseMessage {
    pub content: String,
}

fn default_negligible() -> String {
    "negligible".to_owned()
}

fn default_action() -> String {
    "create".to_owned()
}

#[derive(Deserialize)]
pub struct ObservationJson {
    #[serde(default = "default_negligible")]
    pub noise_level: String,
    pub noise_reason: Option<String>,
    #[serde(rename = "type", default, deserialize_with = "null_as_default")]
    pub observation_type: String,
    #[serde(default, deserialize_with = "null_as_default")]
    pub title: String,
    pub subtitle: Option<String>,
    pub narrative: Option<String>,
    #[serde(default, deserialize_with = "null_or_invalid_as_default_vec")]
    pub facts: Vec<String>,
    #[serde(default, deserialize_with = "null_or_invalid_as_default_vec")]
    pub concepts: Vec<String>,
    #[serde(default, deserialize_with = "null_or_invalid_as_default_vec")]
    pub files_read: Vec<String>,
    #[serde(default, deserialize_with = "null_or_invalid_as_default_vec")]
    pub files_modified: Vec<String>,
    #[serde(default, deserialize_with = "null_or_invalid_as_default_vec")]
    pub keywords: Vec<String>,
    /// Why this observation type was chosen (anti-default justification)
    #[serde(default)]
    pub type_reason: Option<String>,
    /// Context-aware compression action: "create", "update", or "skip"
    #[serde(default = "default_action")]
    pub action: String,
    /// UUID of existing observation to update (only for action="update") — legacy field
    #[serde(default)]
    pub target_id: Option<String>,
    /// 1-based index of existing observation to update (only for action="update")
    #[serde(default)]
    pub target_number: Option<u32>,
    /// Reason for skipping (only for action="skip")
    #[serde(default)]
    pub skip_reason: Option<String>,
}

/// Structured session summary response from LLM.
///
/// Maps directly to the `SessionSummary` storage fields so that each
/// column receives purpose-specific content instead of free-text dumped
/// into a single `learned` column.
#[derive(Debug, Deserialize)]
pub struct StructuredSummaryJson {
    /// Narrative overview (2-3 sentences).
    #[serde(default, deserialize_with = "null_as_default")]
    pub summary: String,
    /// What the user originally requested.
    #[serde(default)]
    pub request: Option<String>,
    /// What was investigated / explored.
    #[serde(default)]
    pub investigated: Option<String>,
    /// What was completed / accomplished.
    #[serde(default)]
    pub completed: Option<String>,
    /// Recommended next steps.
    #[serde(default)]
    pub next_steps: Option<String>,
    /// Files that were read during the session.
    #[serde(default, deserialize_with = "null_or_invalid_as_default_vec")]
    pub files_read: Vec<String>,
    /// Files that were modified during the session.
    #[serde(default, deserialize_with = "null_or_invalid_as_default_vec")]
    pub files_modified: Vec<String>,
    /// Key architectural / design decisions made.
    #[serde(default, deserialize_with = "null_or_invalid_as_default_vec")]
    pub decisions: Vec<String>,
    /// Gotchas, findings, and learned facts.
    #[serde(default, deserialize_with = "null_or_invalid_as_default_vec")]
    pub discoveries: Vec<String>,
}

impl StructuredSummaryJson {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            summary: "No observations in this session.".to_owned(),
            request: None,
            investigated: None,
            completed: None,
            next_steps: None,
            files_read: Vec::new(),
            files_modified: Vec::new(),
            decisions: Vec::new(),
            discoveries: Vec::new(),
        }
    }

    #[must_use]
    pub fn notes_text(&self) -> Option<String> {
        let mut parts = Vec::new();
        if !self.decisions.is_empty() {
            parts.push(format!("Decisions: {}", self.decisions.join("; ")));
        }
        if !self.discoveries.is_empty() {
            parts.push(format!("Discoveries: {}", self.discoveries.join("; ")));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n"))
        }
    }
}

/// LLM response for metadata enrichment of save_memory observations.
#[derive(Deserialize)]
pub struct MetadataJson {
    #[serde(default, deserialize_with = "null_or_invalid_as_default_vec")]
    pub facts: Vec<String>,
    #[serde(default, deserialize_with = "null_or_invalid_as_default_vec")]
    pub concepts: Vec<String>,
    #[serde(default, deserialize_with = "null_or_invalid_as_default_vec")]
    pub keywords: Vec<String>,
    #[serde(default, deserialize_with = "null_or_invalid_as_default_vec")]
    pub files_read: Vec<String>,
    #[serde(default, deserialize_with = "null_or_invalid_as_default_vec")]
    pub files_modified: Vec<String>,
    /// LLM-classified observation type (e.g. "bugfix", "gotcha", "decision").
    #[serde(default, deserialize_with = "null_as_default")]
    pub observation_type: String,
    /// LLM-classified noise level (e.g. "critical", "high", "medium").
    #[serde(default, deserialize_with = "null_as_default")]
    pub noise_level: String,
}
