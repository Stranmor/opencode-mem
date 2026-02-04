use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub(crate) struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub response_format: ResponseFormat,
}

#[derive(Serialize)]
pub(crate) struct ResponseFormat {
    #[serde(rename = "type")]
    pub format_type: String,
}

#[derive(Serialize)]
pub(crate) struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
pub(crate) struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize)]
pub(crate) struct Choice {
    pub message: ResponseMessage,
}

#[derive(Deserialize)]
pub(crate) struct ResponseMessage {
    pub content: String,
}

#[derive(Deserialize)]
pub(crate) struct ObservationJson {
    #[serde(rename = "type")]
    pub observation_type: String,
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
pub(crate) struct SummaryJson {
    pub summary: String,
}
