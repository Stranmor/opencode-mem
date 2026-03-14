use chrono::Utc;
use opencode_mem_core::{
    Concept, NoiseLevel, Observation, ObservationInput, ObservationMetadata, ObservationType,
    sanitize_input,
};
use std::str::FromStr as _;

use crate::ai_types::{
    ChatRequest, Message, MetadataJson, ObservationJson, ResponseFormat, ResponseFormatType,
};
use crate::client::LlmClient;
use crate::compression_prompt::build_compression_prompt;
use crate::error::LlmError;

/// Result of context-aware LLM compression: create new, update existing, or skip.
#[derive(Debug)]
pub enum CompressionResult {
    Create(Observation),
    Update {
        target_id: String,
        observation: Observation,
    },
    Skip {
        reason: String,
    },
}

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
                opencode_mem_core::truncate(response, 300)
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

    let noise_level = NoiseLevel::from_str(&obs_json.noise_level).map_err(|_| {
        tracing::warn!(
            invalid_level = %obs_json.noise_level,
            "LLM returned unknown noise level"
        );
        LlmError::MissingField(format!("unknown noise level: {}", obs_json.noise_level))
    })?;
    if noise_level == NoiseLevel::Negligible {
        let reason = obs_json
            .noise_reason
            .unwrap_or_else(|| "negligible noise level".to_owned());
        tracing::debug!(title = %obs_json.title, "Negligible noise → skip");
        return Ok(CompressionResult::Skip { reason });
    }
    tracing::debug!(
        "Observation noise_level={:?}, reason={:?}, type={}, type_reason={:?}, title={}",
        noise_level,
        obs_json.noise_reason,
        obs_json.observation_type,
        obs_json.type_reason,
        obs_json.title
    );

    let concepts: Vec<Concept> = obs_json
        .concepts
        .iter()
        .filter_map(|s| Concept::from_str(s).ok())
        .collect();

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
    .maybe_project(project.map(|p| p.into()))
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
                return Ok(CompressionResult::Update {
                    target_id: target_id.clone(),
                    observation,
                });
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
            messages: vec![Message {
                role: "user".to_owned(),
                content: prompt,
            }],
            response_format: ResponseFormat {
                format_type: ResponseFormatType::JsonObject,
            },
            max_tokens: None,
        };

        let candidate_ids: std::collections::HashSet<&str> =
            candidates.iter().map(|o| o.id.as_ref()).collect();

        let response = self.chat_completion(&request).await?;
        parse_observation_response(
            &response,
            id,
            input.session_id.as_ref(),
            project,
            &candidate_ids,
        )
    }

    /// Extract structured metadata from an observation's title and narrative.
    ///
    /// # Errors
    /// Returns an error if the API call or JSON parsing fails.
    pub async fn enrich_observation_metadata(
        &self,
        title: &str,
        narrative: &str,
    ) -> Result<ObservationMetadata, LlmError> {
        let prompt = format!(
            "Extract structured metadata from this observation.\n\n\
             Title: {title}\n\
             Narrative: {narrative}\n\n\
             Return JSON with these fields:\n\
             - \"facts\": array of specific facts (file paths, function names, decisions, concrete details)\n\
             - \"concepts\": array from [{concepts}]\n\
             - \"keywords\": array of search keywords (3-8 terms)\n\
             - \"files_read\": array of file paths mentioned as read/referenced\n\
             - \"files_modified\": array of file paths mentioned as modified/created\n\n\
             If a field has no relevant data, return an empty array.",
            concepts = Concept::ALL_VARIANTS_STR,
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

        let response = self.chat_completion(&request).await?;
        let stripped = opencode_mem_core::strip_markdown_json(&response);
        let meta: MetadataJson =
            serde_json::from_str(stripped).map_err(|e| LlmError::JsonParse {
                context: format!(
                    "metadata enrichment (content: {})",
                    opencode_mem_core::truncate(&response, 300)
                ),
                source: e,
            })?;

        let concepts = meta
            .concepts
            .iter()
            .filter_map(|s| Concept::from_str(s).ok())
            .collect();

        Ok(ObservationMetadata {
            facts: meta.facts,
            concepts,
            keywords: meta.keywords,
            files_read: meta.files_read,
            files_modified: meta.files_modified,
        })
    }
}
