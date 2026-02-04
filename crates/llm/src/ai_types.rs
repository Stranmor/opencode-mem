use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub response_format: ResponseFormat,
}

#[derive(Serialize)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub format_type: String,
}

#[derive(Serialize)]
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

const fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct ObservationJson {
    #[serde(default = "default_true")]
    pub should_save: bool,
    #[serde(rename = "type", default)]
    pub observation_type: String,
    #[serde(default)]
    pub title: String,
    pub subtitle: Option<String>,
    pub narrative: Option<String>,
    #[serde(default)]
    pub facts: Vec<String>,
    #[serde(default)]
    pub concepts: Vec<String>,
    #[serde(default)]
    pub files_read: Vec<String>,
    #[serde(default)]
    pub files_modified: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

#[derive(Deserialize)]
pub struct SummaryJson {
    pub summary: String,
}
