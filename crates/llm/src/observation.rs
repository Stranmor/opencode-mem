use chrono::Utc;
use opencode_mem_core::{
    filter_private_content, strip_markdown_json, Concept, Error, Observation, ObservationInput,
    ObservationType, Result,
};
use std::str::FromStr;

use crate::ai_types::{ChatRequest, ChatResponse, Message, ObservationJson, ResponseFormat};
use crate::client::{truncate, LlmClient, MAX_OUTPUT_LEN};

pub(crate) fn parse_concept(s: &str) -> Option<Concept> {
    match s.to_lowercase().as_str() {
        "how-it-works" => Some(Concept::HowItWorks),
        "why-it-exists" => Some(Concept::WhyItExists),
        "what-changed" => Some(Concept::WhatChanged),
        "problem-solution" => Some(Concept::ProblemSolution),
        "gotcha" => Some(Concept::Gotcha),
        "pattern" => Some(Concept::Pattern),
        "trade-off" => Some(Concept::TradeOff),
        _ => None,
    }
}

impl LlmClient {
    pub async fn compress_to_observation(
        &self,
        id: &str,
        input: &ObservationInput,
        project: Option<&str>,
    ) -> Result<Option<Observation>> {
        let filtered_output = filter_private_content(&input.output.output);
        let filtered_title = filter_private_content(&input.output.title);

        let prompt = format!(
            r#"Analyze this tool execution and decide if it's worth remembering.

Tool: {}
Output Title: {}
Output Content: {}

FIRST DECIDE: Is this event worth saving for future reference?

SAVE if it contains:
- Bug fix with solution (how a problem was solved)
- Pattern or technique that can be reused
- Gotcha or pitfall to avoid
- Important decision with reasoning
- New feature implementation details
- Useful discovery about a library/tool/API

DO NOT SAVE trivial events like:
- "Command executed successfully" with no useful output
- Simple file reads without insights
- Routine operations (git status, ls, etc.)
- Failed commands that don't teach anything
- Meta-observations about the system itself

Return JSON:
- should_save: boolean - false if trivial, true if worth remembering
- type: one of "bugfix", "feature", "refactor", "change", "discovery", "decision"
- title: concise title (max 100 chars)
- subtitle: optional one-line context
- narrative: 2-3 sentence explanation of what was learned and why it matters
- facts: array of specific actionable facts
- concepts: array from ["how-it-works", "why-it-exists", "what-changed", "problem-solution", "gotcha", "pattern", "trade-off"]
- files_read: array of file paths
- files_modified: array of file paths
- keywords: 5-10 semantic keywords for search"#,
            input.tool,
            filtered_title,
            truncate(&filtered_output, MAX_OUTPUT_LEN),
        );

        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
            response_format: ResponseFormat {
                format_type: "json_object".to_string(),
            },
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
        let body = response
            .text()
            .await
            .map_err(|e| Error::LlmApi(e.to_string()))?;

        if !status.is_success() {
            return Err(Error::LlmApi(format!("API error {}: {}", status, body)));
        }

        let chat_response: ChatResponse = serde_json::from_str(&body).map_err(|e| {
            Error::LlmApi(format!(
                "Failed to parse response: {} - body: {}",
                e,
                &body[..body.len().min(500)]
            ))
        })?;

        let first_choice = chat_response
            .choices
            .first()
            .ok_or_else(|| Error::LlmApi("API returned empty choices array".to_string()))?;

        let content = strip_markdown_json(&first_choice.message.content);
        let obs_json: ObservationJson = serde_json::from_str(content).map_err(|e| {
            Error::LlmApi(format!(
                "Failed to parse observation JSON: {} - content: {}",
                e,
                &content[..content.len().min(300)]
            ))
        })?;

        if !obs_json.should_save {
            tracing::debug!("Skipping trivial observation: {}", obs_json.title);
            return Ok(None);
        }

        let mut concepts = Vec::new();
        for s in &obs_json.concepts {
            if let Some(concept) = parse_concept(s) {
                concepts.push(concept);
            }
        }

        Ok(Some(Observation {
            id: id.to_string(),
            session_id: input.session_id.clone(),
            project: project.map(|s| s.to_string()),
            observation_type: ObservationType::from_str(&obs_json.observation_type).map_err(
                |_| {
                    Error::InvalidInput(format!(
                        "Invalid observation type: {}",
                        obs_json.observation_type
                    ))
                },
            )?,
            title: obs_json.title,
            subtitle: obs_json.subtitle,
            narrative: obs_json.narrative,
            facts: obs_json.facts,
            concepts,
            files_read: obs_json.files_read,
            files_modified: obs_json.files_modified,
            keywords: obs_json.keywords,
            prompt_number: None,
            discovery_tokens: None,
            created_at: Utc::now(),
        }))
    }
}
