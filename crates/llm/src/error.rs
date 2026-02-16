//! Typed error enum for the LLM crate.

use thiserror::Error;

/// Errors from LLM API operations.
#[derive(Debug, Error)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    HttpRequest(#[from] reqwest::Error),
    #[error("HTTP status {code}: {body}")]
    HttpStatus { code: u16, body: String },
    #[error("JSON parse error in {context}: {source}")]
    JsonParse {
        context: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("empty response: no choices returned")]
    EmptyResponse,
    #[error("missing field in response: {0}")]
    MissingField(String),
    #[error("client initialization failed: {0}")]
    ClientInit(String),
    #[error("all retries exhausted, last error: {0}")]
    RetriesExhausted(Box<LlmError>),
}

impl LlmError {
    /// Whether this error is transient and should be retried.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        match self {
            Self::HttpRequest(_) => true,
            Self::HttpStatus { code, .. } => matches!(code, 429 | 500 | 502 | 503 | 529),
            Self::RetriesExhausted(_) => false,
            _ => false,
        }
    }
}
