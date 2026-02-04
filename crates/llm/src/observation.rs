use chrono::Utc;
use opencode_mem_core::{
    filter_private_content, strip_markdown_json, Concept, Error, Observation, ObservationInput,
    ObservationType, Result,
};
use std::str::FromStr as _;

use crate::ai_types::{ChatRequest, ChatResponse, Message, ObservationJson, ResponseFormat};
use crate::client::{truncate, LlmClient, MAX_OUTPUT_LEN};

#[must_use]
pub fn parse_concept(s: &str) -> Option<Concept> {
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
    /// Compress tool output into an observation using LLM.
    ///
    /// # Errors
    /// Returns `Error::LlmApi` if the API call fails or response parsing fails.
    /// Returns `Error::InvalidInput` if the observation type is invalid.
    #[expect(clippy::too_many_lines, reason = "LLM prompt construction requires sequential steps")]
    pub async fn compress_to_observation(
        &self,
        id: &str,
        input: &ObservationInput,
        project: Option<&str>,
    ) -> Result<Option<Observation>> {
        let filtered_output = filter_private_content(&input.output.output);
        let filtered_title = filter_private_content(&input.output.title);

        let prompt = format!(
            r#"Analyze this tool execution. Decide if it contains PROJECT-SPECIFIC knowledge worth remembering.

Tool: {}
Output Title: {}
Output Content: {}

SAVE ONLY if it contains:
- PROJECT DECISION: Why this project chose X over Y (architecture, library, pattern)
- PROJECT-SPECIFIC GOTCHA: Something unique to THIS codebase/API that would bite someone new
- USER PREFERENCE: How this user likes things done (code style, communication, workflow)
- UNIQUE DISCOVERY: Something about THIS project's dependencies/APIs that isn't obvious

DO NOT SAVE (LLM already knows these):
- Generic programming patterns (async/await, error handling, mutex usage)
- Common library usage (how to use tokio, reqwest, serde)
- Standard fixes (race condition -> RwLock, null check, etc.)
- Routine operations (file reads, git commands, builds)
- Obvious facts (Rust needs cargo, Python needs pip)

CRITICAL: If a senior Rust developer would already know this -> DO NOT SAVE.
Only save what is UNIQUE to this project/user/codebase.

Return JSON:
- should_save: boolean - true ONLY for project-specific knowledge
- type: one of "decision", "discovery", "gotcha", "preference", "change"
- title: what was learned (max 80 chars)
- subtitle: project/context this applies to
- narrative: why this matters FOR THIS PROJECT specifically
- facts: actionable project-specific facts
- concepts: from ["how-it-works", "why-it-exists", "what-changed", "problem-solution", "gotcha", "pattern", "trade-off"]
- files_read: file paths
- files_modified: file paths
- keywords: project-specific terms for search"#,
            input.tool,
            filtered_title,
            truncate(&filtered_output, MAX_OUTPUT_LEN),
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

        let first_choice = chat_response
            .choices
            .first()
            .ok_or_else(|| Error::LlmApi("API returned empty choices array".to_owned()))?;

        let content = strip_markdown_json(&first_choice.message.content);
        let obs_json: ObservationJson = serde_json::from_str(content).map_err(|e| {
            Error::LlmApi(format!(
                "Failed to parse observation JSON: {e} - content: {}",
                content.get(..300).unwrap_or(content)
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
            id: id.to_owned(),
            session_id: input.session_id.clone(),
            project: project.map(ToOwned::to_owned),
            observation_type: ObservationType::from_str(&obs_json.observation_type).map_err(
                |_ignored| {
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
