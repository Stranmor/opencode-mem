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
            r#"You are a STRICT memory filter. Your job is to decide if this tool output contains a LESSON WORTH REMEMBERING across sessions.

Tool: {}
Output Title: {}
Output Content: {}

ONLY SAVE observations that match ONE of these categories:

1. GOTCHA: Something that broke, surprised you, or behaved unexpectedly.
   "SQLite ALTER TABLE does not support adding STORED generated columns"
   "Claude thinking blocks cause Vertex AI API rejection"
   "Lock ordering must be assignments→usage→proxy_pool to avoid deadlocks"

2. BUGFIX: A bug was found AND fixed. What was wrong, why, and how it was solved.
   "Advisory lock leak on connection drop — fixed with after_release hook"
   "Stream timeout sent end_turn causing agents to stop — changed to max_tokens"

3. DECISION (critical only): An irreversible architectural choice with clear reasoning.
   "Chose sqlite-vec over ChromaDB for vector storage — no external dependency"
   "Hermes Core uses isolation-only design to prevent Telegram account bans"

4. FEATURE (critical only): A significant new capability was completed.
   "Implemented hybrid search: FTS5 BM25 50% + vector cosine similarity 50%"

EVERYTHING ELSE IS NEGLIGIBLE. Specifically, ALWAYS mark as negligible:
- Reading/writing files (routine work, not a lesson)
- Code structure descriptions ("module X exports Y") — that's what code is for
- Build/test output (pass or fail)
- Refactoring without a lesson ("extracted method X")
- Configuration changes without a gotcha
- Generic programming knowledge (how async works, how to use serde)
- Discovering how existing code works (read the code next time)
- Status updates, progress reports, task lists
- Metadata about the system itself ("database has N records")

THE DEFAULT IS NEGLIGIBLE. When in doubt, discard. Only save what would genuinely help a future agent avoid a mistake or understand a non-obvious decision.

Return JSON:
- noise_level: "critical", "high", or "negligible" (NO medium/low — binary choice: worth saving or not)
- noise_reason: why this is/isn't worth remembering (max 100 chars)
- type: "gotcha", "bugfix", "decision", or "feature"
- title: the lesson learned (max 80 chars, must be a complete statement of fact)
- subtitle: project/context this applies to
- narrative: the full lesson — what happened, why, and what to do differently
- facts: specific actionable facts (file paths, commands, error messages)
- concepts: from ["problem-solution", "gotcha", "pattern", "trade-off"]
- files_read: file paths involved
- files_modified: file paths changed
- keywords: search terms"#,
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
        let stripped = opencode_mem_core::strip_markdown_json(&content);
        let obs_json: ObservationJson = serde_json::from_str(stripped).map_err(|e| {
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
