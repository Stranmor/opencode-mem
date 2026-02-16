use chrono::Utc;
use opencode_mem_core::{
    filter_private_content, Concept, NoiseLevel, Observation, ObservationInput, ObservationType,
};
use std::str::FromStr as _;

use crate::ai_types::{ChatRequest, Message, ObservationJson, ResponseFormat};
use crate::client::{truncate, LlmClient, MAX_OUTPUT_LEN};
use crate::error::LlmError;

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

/// Build the LLM prompt for compressing tool output into an observation.
///
/// Returns the complete user-message prompt string including tool context,
/// dedup context from existing titles, noise level guide, and JSON schema.
fn build_compression_prompt(
    tool: &str,
    title: &str,
    output: &str,
    existing_titles: &[String],
) -> String {
    let existing_context = if existing_titles.is_empty() {
        String::new()
    } else {
        let titles_list: String = existing_titles
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{}. {}", i.saturating_add(1), t))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            r#"

ALREADY SAVED OBSERVATIONS (similar to this input):
{titles_list}

If this tool output teaches essentially the SAME lesson as any observation above, mark noise_level as "negligible" with noise_reason "duplicate of existing observation". Only save if this adds a genuinely NEW insight not covered above."#
        )
    };

    format!(
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
{}
NOISE LEVEL GUIDE (5 levels):
- "critical": Project-breaking gotcha, data loss bug, security fix. Must ALWAYS be remembered.
- "high": Important bugfix, architectural decision, significant gotcha. Should be remembered.
- "medium": Useful but not critical. Minor gotcha, routine feature completion.
- "low": Marginally useful. Could be helpful but not essential.
- "negligible": Routine work, generic knowledge, duplicates. DISCARD.

Return JSON:
- noise_level: one of "critical", "high", "medium", "low", "negligible"
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
        tool,
        title,
        truncate(output, MAX_OUTPUT_LEN),
        existing_context,
    )
}

/// Parse LLM JSON response into an `Observation`, or `None` if negligible.
///
/// Deserializes the raw LLM response, checks noise level, and builds a typed
/// `Observation` from the parsed fields.
///
/// # Errors
/// Returns an error if JSON deserialization fails or the observation type is invalid.
fn parse_observation_response(
    response: &str,
    id: &str,
    session_id: &str,
    project: Option<&str>,
) -> Result<Option<Observation>, LlmError> {
    let stripped = opencode_mem_core::strip_markdown_json(response);
    let obs_json: ObservationJson =
        serde_json::from_str(stripped).map_err(|e| LlmError::JsonParse {
            context: format!(
                "observation response (content: {})",
                response.get(..300).unwrap_or(response)
            ),
            source: e,
        })?;

    let noise_level = NoiseLevel::from_str(&obs_json.noise_level).unwrap_or_else(|_| {
        tracing::warn!(
            invalid_level = %obs_json.noise_level,
            "LLM returned unknown noise level, defaulting to Normal"
        );
        NoiseLevel::default()
    });
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

    let concepts: Vec<Concept> =
        obs_json.concepts.iter().filter_map(|s| parse_concept(s)).collect();

    let observation_type = ObservationType::from_str(&obs_json.observation_type).map_err(|e| {
        LlmError::MissingField(format!(
            "invalid observation type '{}': {e}",
            obs_json.observation_type
        ))
    })?;

    Ok(Some(
        Observation::builder(
            id.to_owned(),
            session_id.to_owned(),
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

impl LlmClient {
    /// Compress tool output into an observation using LLM.
    ///
    /// Orchestrates three phases: prompt construction, LLM call, response parsing.
    /// Returns `None` if the LLM classifies the output as negligible noise.
    ///
    /// # Errors
    /// Returns an error if the API call fails or response parsing fails.
    pub async fn compress_to_observation(
        &self,
        id: &str,
        input: &ObservationInput,
        project: Option<&str>,
        existing_titles: &[String],
    ) -> Result<Option<Observation>, LlmError> {
        let filtered_output = filter_private_content(&input.output.output);
        let filtered_title = filter_private_content(&input.output.title);

        let prompt = build_compression_prompt(
            &input.tool,
            &filtered_title,
            &filtered_output,
            existing_titles,
        );

        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![Message { role: "user".to_owned(), content: prompt }],
            response_format: ResponseFormat { format_type: "json_object".to_owned() },
        };

        let response = self.chat_completion(&request).await?;
        parse_observation_response(&response, id, &input.session_id, project)
    }
}
