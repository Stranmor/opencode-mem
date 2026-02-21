use chrono::Utc;
use opencode_mem_core::{
    sanitize_input, Concept, NoiseLevel, Observation, ObservationInput, ObservationType,
};
use std::str::FromStr as _;

use crate::ai_types::{ChatRequest, Message, ObservationJson, ResponseFormat};
use crate::client::{truncate, LlmClient, MAX_OUTPUT_LEN};
use crate::error::LlmError;

/// Result of context-aware LLM compression: create new, update existing, or skip.
#[derive(Debug)]
pub enum CompressionResult {
    Create(Observation),
    Update { target_id: String, observation: Observation },
    Skip { reason: String },
}

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
/// candidate observations for context-aware dedup, noise level guide, and JSON schema.
fn build_compression_prompt(
    tool: &str,
    title: &str,
    output: &str,
    candidates: &[Observation],
) -> String {
    let mut types_prompt = String::new();
    for (i, variant) in opencode_mem_core::ObservationType::ALL_VARIANTS.iter().enumerate() {
        types_prompt.push_str(&format!(
            "{}. {}: {}\n",
            i.saturating_add(1),
            variant.as_str().to_uppercase(),
            variant.description()
        ));
        for example in variant.examples() {
            types_prompt.push_str(&format!("   {}\n", example));
        }
        types_prompt.push('\n');
    }

    let existing_context = if candidates.is_empty() {
        "\n\nThere are no existing observations. You MUST use action: \"create\".".to_owned()
    } else {
        let mut entries = String::new();
        for (i, obs) in candidates.iter().enumerate() {
            let narrative_preview =
                obs.narrative.as_deref().unwrap_or("").chars().take(200).collect::<String>();
            entries.push_str(&format!(
                "[{}] id=\"{}\" title=\"{}\" | {}\n",
                i.saturating_add(1),
                obs.id,
                obs.title,
                narrative_preview,
            ));
        }
        format!(
            r#"

EXISTING OBSERVATIONS (potentially related):
{entries}
DECISION (MANDATORY — choose exactly one):
- If this is genuinely NEW knowledge not covered by any existing observation → action: "create"
- If this REFINES or ADDS TO an existing observation above → action: "update", target_id: "<id of the observation to update>"
- If this adds ZERO new information beyond what already exists → action: "skip""#
        )
    };

    let json_schema = if candidates.is_empty() {
        format!(
            r#"Return JSON:
- action: "create"
- noise_level: one of [{noise_levels}]
- noise_reason: why this is/isn't worth remembering (max 100 chars)
- type: one of [{obs_types}]
- title: the lesson learned (max 80 chars, must be a complete statement of fact)
- subtitle: project/context this applies to
- narrative: the full lesson — what happened, why, and what to do differently
- facts: specific actionable facts (file paths, commands, error messages)
- concepts: from [{concepts}]
- files_read: file paths involved
- files_modified: file paths changed
- keywords: search terms"#,
            obs_types = opencode_mem_core::ObservationType::ALL_VARIANTS_STR,
            noise_levels = opencode_mem_core::NoiseLevel::ALL_VARIANTS_STR,
            concepts = opencode_mem_core::Concept::ALL_VARIANTS_STR
        )
    } else {
        format!(
            r#"Return JSON:
- action: one of "create", "update", "skip"
- target_id: id of existing observation to update (required if action is "update")
- skip_reason: why this should be skipped (required if action is "skip")
- noise_level: one of [{noise_levels}]
- noise_reason: why this is/isn't worth remembering (max 100 chars)
- type: one of [{obs_types}]
- title: the lesson learned (max 80 chars, must be a complete statement of fact)
- subtitle: project/context this applies to
- narrative: the full lesson — what happened, why, and what to do differently
- facts: specific actionable facts (file paths, commands, error messages)
- concepts: from [{concepts}]
- files_read: file paths involved
- files_modified: file paths changed
- keywords: search terms"#,
            obs_types = opencode_mem_core::ObservationType::ALL_VARIANTS_STR,
            noise_levels = opencode_mem_core::NoiseLevel::ALL_VARIANTS_STR,
            concepts = opencode_mem_core::Concept::ALL_VARIANTS_STR
        )
    };

    format!(
        r#"You are a STRICT memory filter. Your job is to decide if this tool output contains a LESSON WORTH REMEMBERING across sessions.

Tool: {}
Output Title: {}
Output Content: {}

ONLY SAVE observations that match ONE of these categories:

{}
EVERYTHING ELSE IS NEGLIGIBLE. Specifically, ALWAYS mark as negligible:
- Reading/writing files (routine work, not a lesson)
- Code structure descriptions ("module X exports Y") — that's what code is for
- Build/test output (pass or fail)
- Status updates, progress reports, task lists
- Metadata about the system itself ("database has N records")

THE DEFAULT IS NEGLIGIBLE. When in doubt, discard. Only save what would genuinely help a future agent avoid a mistake or understand a non-obvious decision.
{}
NOISE LEVEL GUIDE (5 levels):
- "critical": Production outage, data loss, security vulnerability, core architectural decision that affects the entire system. If ignoring this would cause system failure, it's critical.
- "high": Important bugfix with root cause analysis, significant gotcha that saves hours of debugging, architectural decision with clear tradeoffs, user preference that affects workflow.
- "medium": Useful operational gotcha, minor bugfix, routine feature completion with a non-obvious implementation detail.
- "low": Marginally useful context. Configuration tweak, minor optimization, environment-specific workaround.
- "negligible": Routine work, generic knowledge available in docs, file edits, build output, status updates, duplicates. DISCARD.

{}"#,
        tool,
        title,
        truncate(output, MAX_OUTPUT_LEN),
        types_prompt,
        existing_context,
        json_schema,
    )
}

/// Parse LLM JSON response into a `CompressionResult`.
///
/// Deserializes the raw LLM response, checks the action field and noise level,
/// and builds either Create, Update, or Skip result.
///
/// The `candidate_ids` set is used to validate that an "update" target_id
/// was actually in the candidate set (hallucination guard).
///
/// # Errors
/// Returns an error if JSON deserialization fails or the observation type is invalid.
fn parse_observation_response(
    response: &str,
    id: &str,
    session_id: &str,
    project: Option<&str>,
    candidate_ids: &std::collections::HashSet<&str>,
) -> Result<CompressionResult, LlmError> {
    let stripped = opencode_mem_core::strip_markdown_json(response);
    let obs_json: ObservationJson =
        serde_json::from_str(stripped).map_err(|e| LlmError::JsonParse {
            context: format!(
                "observation response (content: {})",
                response.get(..300).unwrap_or(response)
            ),
            source: e,
        })?;

    let action = obs_json.action.to_lowercase();

    // Skip action — return early regardless of noise_level
    if action == "skip" {
        let reason = obs_json
            .skip_reason
            .or(obs_json.noise_reason)
            .unwrap_or_else(|| "LLM decided to skip".to_owned());
        tracing::info!(reason = %reason, "LLM action: skip");
        return Ok(CompressionResult::Skip { reason });
    }

    let noise_level = NoiseLevel::from_str(&obs_json.noise_level).unwrap_or_else(|_| {
        tracing::warn!(
            invalid_level = %obs_json.noise_level,
            "LLM returned unknown noise level, defaulting to Normal"
        );
        NoiseLevel::default()
    });
    if noise_level == NoiseLevel::Negligible {
        let reason = obs_json.noise_reason.unwrap_or_else(|| "negligible noise level".to_owned());
        tracing::debug!(title = %obs_json.title, "Negligible noise → skip");
        return Ok(CompressionResult::Skip { reason });
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

    let observation = Observation::builder(
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
    .build();

    // Determine action: update requires valid target_id in candidate set
    if action == "update" {
        if let Some(ref target_id) = obs_json.target_id {
            if candidate_ids.contains(target_id.as_str()) {
                return Ok(CompressionResult::Update { target_id: target_id.clone(), observation });
            }
            tracing::warn!(
                target_id = %target_id,
                "LLM returned update with target_id not in candidate set — treating as create"
            );
        } else {
            tracing::warn!("LLM returned action=update without target_id — treating as create");
        }
    }

    if action != "create" {
        tracing::warn!(action = %action, "LLM returned unrecognized action — treating as create");
    }

    Ok(CompressionResult::Create(observation))
}

impl LlmClient {
    /// Compress tool output into an observation using context-aware LLM compression.
    ///
    /// Accepts candidate observations for dedup context. The LLM decides whether
    /// to CREATE new, UPDATE existing, or SKIP.
    ///
    /// # Errors
    /// Returns an error if the API call fails or response parsing fails.
    pub async fn compress_to_observation(
        &self,
        id: &str,
        input: &ObservationInput,
        project: Option<&str>,
        candidates: &[Observation],
    ) -> Result<CompressionResult, LlmError> {
        let filtered_output = sanitize_input(&input.output.output);
        let filtered_title = sanitize_input(&input.output.title);

        let prompt =
            build_compression_prompt(&input.tool, &filtered_title, &filtered_output, candidates);

        let request = ChatRequest {
            model: self.model(),
            messages: vec![Message { role: "user".to_owned(), content: prompt }],
            response_format: ResponseFormat { format_type: "json_object".to_owned() },
            max_tokens: None,
        };

        let candidate_ids: std::collections::HashSet<&str> =
            candidates.iter().map(|o| o.id.as_str()).collect();

        let response = self.chat_completion(&request).await?;
        parse_observation_response(&response, id, &input.session_id, project, &candidate_ids)
    }
}
