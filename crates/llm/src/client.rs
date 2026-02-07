use opencode_mem_core::{strip_markdown_json, Error, Result};

use crate::ai_types::{ChatRequest, ChatResponse};

/// Maximum output length for truncation.
pub const MAX_OUTPUT_LEN: usize = 2000;
/// Default LLM model to use.
pub const DEFAULT_MODEL: &str = "gemini-3-pro-high";

/// Client for LLM API calls.
#[derive(Debug)]
pub struct LlmClient {
    pub(crate) client: reqwest::Client,
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    pub(crate) model: String,
}

impl LlmClient {
    /// Creates a new LLM client with the given API key and base URL.
    #[must_use]
    pub fn new(api_key: String, base_url: String) -> Self {
        Self { client: reqwest::Client::new(), api_key, base_url, model: DEFAULT_MODEL.to_owned() }
    }

    /// Sets a custom model for this client.
    #[must_use]
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    /// Returns a reference to the underlying HTTP client.
    #[must_use]
    pub const fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Returns the base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns the API key.
    #[must_use]
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Returns the model name.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Send a chat completion request and return the extracted content string.
    ///
    /// # Errors
    /// Returns `Error::LlmApi` if the HTTP request fails, the API returns a
    /// non-success status, the response body cannot be parsed, or the choices
    /// array is empty.
    pub async fn chat_completion(&self, request: &ChatRequest) -> Result<String> {
        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(request)
            .send()
            .await
            .map_err(|e| Error::LlmApi(e.to_string()))?;

        let status = response.status();
        let body = response.text().await.map_err(|e| Error::LlmApi(e.to_string()))?;

        if !status.is_success() {
            return Err(Error::LlmApi(format!("API error {status}: {body}")));
        }

        let chat_response: ChatResponse = serde_json::from_str(&body).map_err(|e| {
            Error::LlmApi(format!(
                "Failed to parse response: {e} - body: {}",
                body.get(..500).unwrap_or(&body)
            ))
        })?;

        let first_choice = chat_response
            .choices
            .first()
            .ok_or_else(|| Error::LlmApi("API returned empty choices array".to_owned()))?;

        Ok(strip_markdown_json(&first_choice.message.content).to_owned())
    }
}

/// Truncates a string to the given maximum length at a char boundary.
#[must_use]
pub fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        s.get(..end).unwrap_or("")
    }
}
