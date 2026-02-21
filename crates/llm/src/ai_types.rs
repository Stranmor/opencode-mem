use serde::{Deserialize, Deserializer, Serialize};

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
    pub format_type: String,
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

fn default_medium() -> String {
    "medium".to_owned()
}

fn default_action() -> String {
    "create".to_owned()
}

#[derive(Deserialize)]
pub struct ObservationJson {
    #[serde(default = "default_medium")]
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
    /// Context-aware compression action: "create", "update", or "skip"
    #[serde(default = "default_action")]
    pub action: String,
    /// UUID of existing observation to update (only for action="update")
    #[serde(default)]
    pub target_id: Option<String>,
    /// Reason for skipping (only for action="skip")
    #[serde(default)]
    pub skip_reason: Option<String>,
}

#[derive(Deserialize)]
pub struct SummaryJson {
    pub summary: String,
}
