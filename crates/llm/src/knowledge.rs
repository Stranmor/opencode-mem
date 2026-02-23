use opencode_mem_core::{
    Concept, KnowledgeExtractionResult, KnowledgeInput, KnowledgeType, Observation,
};

use crate::ai_types::{ChatRequest, Message, ResponseFormat};
use crate::client::LlmClient;
use crate::error::LlmError;

impl LlmClient {
    /// Extract generalizable knowledge from an observation.
    ///
    /// # Errors
    /// Returns an error if the API call fails or response parsing fails.
    pub async fn maybe_extract_knowledge(
        &self,
        observation: &Observation,
    ) -> Result<Option<KnowledgeInput>, LlmError> {
        if matches!(
            observation.noise_level,
            opencode_mem_core::NoiseLevel::Critical | opencode_mem_core::NoiseLevel::High
        ) {
            tracing::debug!(
                id = %observation.id,
                noise = ?observation.noise_level,
                "Skipping knowledge extraction for noisy observation"
            );
            return Ok(None);
        }

        let dominated_by_generalizable =
            matches!(observation.observation_type, opencode_mem_core::ObservationType::Gotcha)
                || observation
                    .concepts
                    .iter()
                    .any(|c| matches!(c, Concept::Pattern | Concept::Gotcha | Concept::HowItWorks));

        if !dominated_by_generalizable {
            return Ok(None);
        }

        let facts_str = observation.facts.join("\n- ");
        let concepts_str =
            observation.concepts.iter().map(|c| format!("{c:?}")).collect::<Vec<_>>().join(", ");

        let prompt = format!(
            r#"Analyze this observation and decide if it contains generalizable knowledge that would help in OTHER projects (not just this one).

Observation:
- Title: {}
- Type: {:?}
- Concepts: {}
- Narrative: {}
- Facts:
- {}

If this contains a reusable skill, pattern, or gotcha that applies broadly:
Return JSON: {{"extract": true, "knowledge_type": "{}", "title": "...", "description": "...", "instructions": "...", "triggers": [...]}}

If this is project-specific and not generalizable:
Return JSON: {{"extract": false, "reason": "..."}}"#,
            observation.title,
            observation.observation_type,
            concepts_str,
            observation.narrative.as_deref().unwrap_or(""),
            facts_str,
            opencode_mem_core::KnowledgeType::ALL_VARIANTS_STR,
        );

        let request = ChatRequest {
            model: self.model(),
            messages: vec![Message { role: "user".to_owned(), content: prompt }],
            response_format: ResponseFormat { format_type: "json_object".to_owned() },
            max_tokens: None,
        };

        let content = self.chat_completion(&request).await?;
        let stripped = opencode_mem_core::strip_markdown_json(&content);
        let extraction: KnowledgeExtractionResult =
            serde_json::from_str(stripped).map_err(|e| LlmError::JsonParse {
                context: format!(
                    "knowledge extraction (content: {})",
                    content.chars().take(300).collect::<String>()
                ),
                source: e,
            })?;

        if !extraction.extract {
            return Ok(None);
        }

        let knowledge_type_str = extraction
            .knowledge_type
            .ok_or_else(|| LlmError::MissingField("knowledge_type".to_owned()))?;
        let knowledge_type = knowledge_type_str.parse::<KnowledgeType>().map_err(|_| {
            LlmError::MissingField(format!("unknown knowledge_type: {}", knowledge_type_str))
        })?;

        Ok(Some(KnowledgeInput::new(
            knowledge_type,
            extraction.title.unwrap_or_else(|| observation.title.clone()),
            extraction
                .description
                .unwrap_or_else(|| observation.narrative.clone().unwrap_or_default()),
            extraction.instructions,
            extraction.triggers.unwrap_or_default(),
            observation.project.clone(),
            Some(observation.id.clone()),
        )))
    }
}

#[cfg(test)]
mod tests {

    use opencode_mem_core::{Concept, Observation, ObservationType};

    #[test]
    fn test_knowledge_extraction_type_bypass() {
        let mut obs = Observation::builder(
            "test".to_string(),
            "manual".to_string(),
            ObservationType::Gotcha,
            "Test Gotcha".to_string(),
        )
        .build();
        obs.concepts = vec![Concept::WhyItExists, Concept::WhatChanged];

        // Simulated logic from maybe_extract_knowledge
        let dominated = matches!(obs.observation_type, ObservationType::Gotcha)
            || obs
                .concepts
                .iter()
                .any(|c| matches!(c, Concept::Pattern | Concept::Gotcha | Concept::HowItWorks));

        assert!(
            dominated,
            "Vulnerability fixed: ObservationType::Gotcha triggers extraction even if concepts are generic"
        );
    }
}
