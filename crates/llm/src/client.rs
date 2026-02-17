use crate::ai_types::{ChatRequest, ChatResponse};
use crate::error::LlmError;

/// Maximum output length for truncation.
pub const MAX_OUTPUT_LEN: usize = 2000;
/// Default LLM model to use.
pub const DEFAULT_MODEL: &str = "gemini-3-pro-high";

/// Client for LLM API calls.
pub struct LlmClient {
    pub(crate) client: reqwest::Client,
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    pub(crate) model: String,
}

impl std::fmt::Debug for LlmClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmClient")
            .field("client", &self.client)
            .field("api_key", &"***")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish()
    }
}

impl LlmClient {
    /// Creates a new LLM client with the given API key and base URL.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be built (TLS backend failure).
    pub fn new(api_key: String, base_url: String) -> Result<Self, LlmError> {
        let model =
            std::env::var("OPENCODE_MEM_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_owned());
        let base_url = base_url.trim_end_matches('/').to_owned();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| LlmError::ClientInit(e.to_string()))?;
        Ok(Self { client, api_key, base_url, model })
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
    /// Returns an error if the HTTP request fails, the API returns a
    /// non-success status, the response body cannot be parsed, or the choices
    /// array is empty.
    pub async fn chat_completion(&self, request: &ChatRequest) -> Result<String, LlmError> {
        const MAX_RETRIES: usize = 3;
        const RETRY_DELAYS: [u64; 4] = [0, 1, 2, 4];
        let mut last_error: Option<LlmError> = None;

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay_secs = RETRY_DELAYS.get(attempt).copied().unwrap_or(4);
                let delay = std::time::Duration::from_secs(delay_secs);
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
                    last_error = Some(LlmError::HttpRequest(e));
                    continue;
                },
            };

            let status = response.status();
            if status.is_success() {
                let body = match response.text().await {
                    Ok(b) => b,
                    Err(e) => {
                        last_error = Some(LlmError::HttpRequest(e));
                        continue;
                    },
                };

                let chat_response: ChatResponse =
                    serde_json::from_str(&body).map_err(|e| LlmError::JsonParse {
                        context: format!(
                            "chat completion response (body: {})",
                            truncate(&body, 200)
                        ),
                        source: e,
                    })?;

                let first_choice = chat_response.choices.first().ok_or(LlmError::EmptyResponse)?;

                return Ok(first_choice.message.content.clone());
            }

            let status_code = status.as_u16();
            let body =
                response.text().await.unwrap_or_else(|_| "Could not read error body".to_string());

            let err = LlmError::HttpStatus { code: status_code, body };
            if err.is_transient() {
                last_error = Some(err);
                continue;
            }
            return Err(err);
        }

        Err(LlmError::RetriesExhausted(Box::new(last_error.unwrap_or(LlmError::EmptyResponse))))
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
