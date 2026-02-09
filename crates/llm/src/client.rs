use opencode_mem_core::{strip_markdown_json, Error, Result};

use crate::ai_types::{ChatRequest, ChatResponse};

/// Maximum output length for truncation.
pub const MAX_OUTPUT_LEN: usize = 2000;
/// Default LLM model to use.
pub const DEFAULT_MODEL: &str = "gemini-3-flash";

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
        let model =
            std::env::var("OPENCODE_MEM_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_owned());
        Self { client: reqwest::Client::new(), api_key, base_url, model }
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
        const MAX_RETRIES: u32 = 3;
        let mut last_error = None;

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(1 << (attempt - 1));
                tokio::time::sleep(delay).await;
                tracing::warn!("LLM retry attempt {attempt}/{MAX_RETRIES} after {delay:?}");
            }

            let response_result = self
                .client
                .post(format!("{}/v1/chat/completions", self.base_url))
                .header("Authorization", format!("Bearer {}", self.api_key))
                .json(request)
                .send()
                .await;

            let response = match response_result {
                Ok(r) => r,
                Err(e) => {
                    last_error = Some(Error::LlmApi(e.to_string()));
                    continue;
                },
            };

            let status = response.status();
            if status.is_success() {
                let body_result = response.text().await;
                let body = match body_result {
                    Ok(b) => b,
                    Err(e) => {
                        last_error = Some(Error::LlmApi(e.to_string()));
                        continue;
                    },
                };

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

                return Ok(strip_markdown_json(&first_choice.message.content).to_owned());
            }

            let status_code = status.as_u16();
            let body =
                response.text().await.unwrap_or_else(|_| "Could not read error body".to_string());
            let err_msg = format!("API error {status}: {body}");

            match status_code {
                429 | 500 | 502 | 503 | 529 => {
                    last_error = Some(Error::LlmApi(err_msg));
                    continue;
                },
                _ => return Err(Error::LlmApi(err_msg)),
            }
        }

        Err(last_error.unwrap_or_else(|| Error::LlmApi("All retries exhausted".to_owned())))
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
