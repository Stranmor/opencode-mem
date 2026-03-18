use opencode_mem_core::Observation;

use crate::ai_types::{
    ChatRequest, Message, ResponseFormat, ResponseFormatType, StructuredSummaryJson,
};
use crate::client::LlmClient;
use crate::error::LlmError;

impl LlmClient {
    /// Generate a structured summary of a coding session from observations.
    ///
    /// # Errors
    /// Returns an error if the API call fails or response parsing fails.
    pub async fn generate_session_summary(
        &self,
        observations: &[Observation],
    ) -> Result<StructuredSummaryJson, LlmError> {
        if observations.is_empty() {
            return Ok(StructuredSummaryJson::empty());
        }

        let obs_text: String = observations
            .iter()
            .map(|o| {
                let mut parts = vec![format!(
                    "- [{}] {}: {}",
                    o.observation_type.as_str(),
                    o.title,
                    o.subtitle.as_deref().unwrap_or("")
                )];
                if let Some(narrative) = &o.narrative {
                    parts.push(format!("  narrative: {narrative}"));
                }
                if !o.files_read.is_empty() {
                    parts.push(format!("  files_read: {}", o.files_read.join(", ")));
                }
                if !o.files_modified.is_empty() {
                    parts.push(format!("  files_modified: {}", o.files_modified.join(", ")));
                }
                parts.join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"Analyze this coding session and return a structured JSON summary.

Observations:
{obs_text}

Return JSON with these fields:
{{
  "summary": "2-3 sentence narrative of the session",
  "request": "what the user originally asked for (null if unclear)",
  "investigated": "what was explored or researched (null if nothing)",
  "completed": "what was accomplished (null if nothing completed)",
  "next_steps": "recommended follow-up actions (null if none)",
  "files_read": ["list", "of", "files", "read"],
  "files_modified": ["list", "of", "files", "modified"],
  "decisions": ["key architectural or design decisions made"],
  "discoveries": ["gotchas, findings, or learned facts"]
}}"#
        );

        let request = ChatRequest {
            model: self.model(),
            messages: vec![Message {
                role: "user".to_owned(),
                content: prompt,
            }],
            response_format: ResponseFormat {
                format_type: ResponseFormatType::JsonObject,
            },
            max_tokens: None,
        };

        let content = self.chat_completion(&request).await?;
        let stripped = opencode_mem_core::strip_markdown_json(&content);
        let summary: StructuredSummaryJson =
            serde_json::from_str(stripped).map_err(|e| LlmError::JsonParse {
                context: format!(
                    "structured session summary (content: {})",
                    opencode_mem_core::truncate(&content, 300)
                ),
                source: e,
            })?;
        Ok(summary)
    }
}
