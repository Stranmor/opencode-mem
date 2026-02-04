use opencode_mem_core::{strip_markdown_json, Error, Observation, Result};

use crate::ai_types::{ChatRequest, ChatResponse, Message, ResponseFormat, SummaryJson};
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

        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
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

        let first_choice = chat_response.choices.first().ok_or_else(|| {
            Error::LlmApi("API returned empty choices array for session summary".to_owned())
        })?;

        let content = strip_markdown_json(&first_choice.message.content);
        let summary: SummaryJson = serde_json::from_str(content).map_err(|e| {
            Error::LlmApi(format!(
                "Failed to parse session summary: {e} - content: {}",
                content.get(..300).unwrap_or(content)
            ))
        })?;
        Ok(summary.summary)
    }
}
