use chrono::Utc;
use opencode_mem_core::{
    filter_private_content, Concept, Error, NoiseLevel, Observation, ObservationInput,
    ObservationType, Result,
};
use std::str::FromStr as _;

use crate::ai_types::{ChatRequest, Message, ObservationJson, ResponseFormat};
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
    pub async fn compress_to_observation(
        &self,
        id: &str,
        input: &ObservationInput,
        project: Option<&str>,
    ) -> Result<Option<Observation>> {
        let filtered_output = filter_private_content(&input.output.output);
        let filtered_title = filter_private_content(&input.output.title);

        let prompt = format!(
            r#"Analyze this tool execution and classify its noise level for memory storage.

Tool: {}
Output Title: {}
Output Content: {}

NOISE LEVELS (from most to least important):
- "critical": Major architectural decisions, breaking changes, security issues
- "high": Project-specific gotchas, user preferences, unique discoveries
- "medium": Useful context that might help later (default)
- "low": Routine operations, common patterns, obvious facts
- "negligible": Pure noise - file listings, build output, trivial reads

CLASSIFY AS CRITICAL/HIGH if it contains:
- PROJECT DECISION: Why this project chose X over Y (architecture, library, pattern)
- PROJECT-SPECIFIC GOTCHA: Something unique to THIS codebase/API
- USER PREFERENCE: How this user likes things done
- UNIQUE DISCOVERY: Something about THIS project's dependencies/APIs

CLASSIFY AS LOW/NEGLIGIBLE if:
- Generic programming patterns (async/await, error handling)
- Common library usage (how to use tokio, reqwest, serde)
- Routine operations (file reads, git commands, builds)
- Obvious facts (Rust needs cargo, Python needs pip)

Return JSON:
- noise_level: one of "critical", "high", "medium", "low", "negligible"
- noise_reason: brief explanation of classification (max 100 chars)
- type: one of "decision", "discovery", "gotcha", "preference", "change", "bugfix", "refactor", "feature"
- title: what was learned (max 80 chars)
- subtitle: project/context this applies to
- narrative: why this matters
- facts: actionable facts
- concepts: from ["how-it-works", "why-it-exists", "what-changed", "problem-solution", "gotcha", "pattern", "trade-off"]
- files_read: file paths
- files_modified: file paths
- keywords: terms for search"#,
            input.tool,
            filtered_title,
            truncate(&filtered_output, MAX_OUTPUT_LEN),
        );

        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![Message { role: "user".to_owned(), content: prompt }],
            response_format: ResponseFormat { format_type: "json_object".to_owned() },
        };

        let content = self.chat_completion(&request).await?;
        let obs_json: ObservationJson = serde_json::from_str(&content).map_err(|e| {
            Error::LlmApi(format!(
                "Failed to parse observation JSON: {e} - content: {}",
                content.get(..300).unwrap_or(&content)
            ))
        })?;

        let noise_level = NoiseLevel::from_str(&obs_json.noise_level).unwrap_or_default();
        if noise_level == NoiseLevel::Negligible {
            tracing::debug!(title = %obs_json.title, "Skipping negligible observation");
            return Ok(None);
        }
        tracing::debug!(
            "Observation noise_level={:?}, reason={:?}, title={}",
            noise_level,
            obs_json.noise_reason,
            obs_json.title
        );

        let mut concepts = Vec::new();
        for s in &obs_json.concepts {
            if let Some(concept) = parse_concept(s) {
                concepts.push(concept);
            }
        }

        let observation_type =
            ObservationType::from_str(&obs_json.observation_type).map_err(|_ignored| {
                Error::InvalidInput(format!(
                    "Invalid observation type: {}",
                    obs_json.observation_type
                ))
            })?;

        Ok(Some(
            Observation::builder(
                id.to_owned(),
                input.session_id.clone(),
                observation_type,
                obs_json.title,
            )
            .maybe_project(project.map(ToOwned::to_owned))
            .maybe_subtitle(obs_json.subtitle)
            .maybe_narrative(obs_json.narrative)
            .facts(obs_json.facts)
            .concepts(concepts)
            .files_read(obs_json.files_read)
            .files_modified(obs_json.files_modified)
            .keywords(obs_json.keywords)
            .noise_level(noise_level)
            .maybe_noise_reason(obs_json.noise_reason)
            .created_at(Utc::now())
            .build(),
        ))
    }
}
