pub const MAX_OUTPUT_LEN: usize = 2000;
pub const DEFAULT_MODEL: &str = "gemini-3-pro-high";

#[derive(Debug)]
pub struct LlmClient {
    pub(crate) client: reqwest::Client,
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    pub(crate) model: String,
}

impl LlmClient {
    #[must_use]
    pub fn new(api_key: String, base_url: String) -> Self {
        Self { client: reqwest::Client::new(), api_key, base_url, model: DEFAULT_MODEL.to_owned() }
    }

    #[must_use]
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    #[must_use]
    pub const fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    #[must_use]
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }
}

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
