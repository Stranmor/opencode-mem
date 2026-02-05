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
