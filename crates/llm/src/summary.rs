use opencode_mem_core::{Error, Observation, Result};

use crate::ai_types::{ChatRequest, Message, ResponseFormat, SummaryJson};
use crate::client::LlmClient;

impl LlmClient {
    /// Generate a summary of a coding session from observations.
    ///
    /// # Errors
    /// Returns `Error::LlmApi` if the API call fails or response parsing fails.
    pub async fn generate_session_summary(&self, observations: &[Observation]) -> Result<String> {
        if observations.is_empty() {
            return Ok("No observations in this session.".to_owned());
        }

        let obs_text: String = observations
            .iter()
            .map(|o| {
                format!(
                    "- [{}] {}: {}",
                    o.observation_type.as_str(),
                    o.title,
                    o.subtitle.as_deref().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"Summarize this coding session based on the observations below.
Write 2-3 sentences highlighting key accomplishments and decisions.

Observations:
{obs_text}

Return JSON: {{"summary": "..."}}"#
        );

        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![Message { role: "user".to_owned(), content: prompt }],
            response_format: ResponseFormat { format_type: "json_object".to_owned() },
        };

        let content = self.chat_completion(&request).await?;
        let stripped = opencode_mem_core::strip_markdown_json(&content);
        let summary: SummaryJson = serde_json::from_str(stripped).map_err(|e| {
            Error::LlmApi(format!(
                "Failed to parse session summary: {e} - content: {}",
                content.get(..300).unwrap_or(&content)
            ))
        })?;
        Ok(summary.summary)
    }
}
