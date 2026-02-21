use std::sync::RwLock;

use crate::ai_types::{ChatRequest, ChatResponse};
use crate::error::LlmError;

/// Maximum output length for truncation.
pub const MAX_OUTPUT_LEN: usize = 2000;
/// Default LLM model to use.
pub const DEFAULT_MODEL: &str = "gemini-3-pro-high";

/// Client for LLM API calls.
pub struct LlmClient {
    pub(crate) client: reqwest::Client,
    pub(crate) api_key: RwLock<String>,
    pub(crate) base_url: RwLock<String>,
    pub(crate) model: RwLock<String>,
}

impl std::fmt::Debug for LlmClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let base_url = self.base_url.read().unwrap_or_else(|e| e.into_inner()).clone();
        let model = self.model.read().unwrap_or_else(|e| e.into_inner()).clone();
        f.debug_struct("LlmClient")
            .field("client", &self.client)
            .field("api_key", &"***")
            .field("base_url", &base_url)
            .field("model", &model)
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
        Ok(Self {
            client,
            api_key: RwLock::new(api_key),
            base_url: RwLock::new(base_url),
            model: RwLock::new(model),
        })
    }

    /// Sets a custom model for this client.
    #[must_use]
    pub fn with_model(self, model: String) -> Self {
        if let Ok(mut m) = self.model.write() {
            *m = model;
        }
        self
    }

    pub fn update_config(
        &self,
        api_key: Option<String>,
        base_url: Option<String>,
        model: Option<String>,
    ) {
        if let Some(key) = api_key {
            if let Ok(mut k) = self.api_key.write() {
                *k = key;
            }
        }
        if let Some(url) = base_url {
            if let Ok(mut u) = self.base_url.write() {
                *u = url.trim_end_matches('/').to_owned();
            }
        }
        if let Some(m) = model {
            if let Ok(mut md) = self.model.write() {
                *md = m;
            }
        }
    }

    /// Returns a reference to the underlying HTTP client.
    #[must_use]
    pub const fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Returns the base URL.
    #[must_use]
    pub fn base_url(&self) -> String {
        self.base_url.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Returns the API key.
    #[must_use]
    pub fn api_key(&self) -> String {
        self.api_key.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Returns the model name.
    #[must_use]
    pub fn model(&self) -> String {
        self.model.read().unwrap_or_else(|e| e.into_inner()).clone()
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

        let base_url = self.base_url();
        let api_key = self.api_key();

        let mut req_body = request.clone();
        if req_body.model.is_empty() {
            req_body.model = self.model();
        }

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay_secs = RETRY_DELAYS.get(attempt).copied().unwrap_or(4);
                let delay = std::time::Duration::from_secs(delay_secs);
                tokio::time::sleep(delay).await;
                tracing::warn!("LLM retry attempt {attempt}/{MAX_RETRIES} after {delay:?}");
            }

            let response_result = self
                .client
                .post(format!("{}/v1/chat/completions", base_url))
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&req_body)
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
