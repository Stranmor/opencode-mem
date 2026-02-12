use opencode_mem_core::{
    Concept, Error, KnowledgeExtractionResult, KnowledgeInput, KnowledgeType, Observation, Result,
};

use crate::ai_types::{ChatRequest, Message, ResponseFormat};
use crate::client::LlmClient;

impl LlmClient {
    /// Extract generalizable knowledge from an observation.
    ///
    /// # Errors
    /// Returns `Error::LlmApi` if the API call fails or response parsing fails.
    pub async fn maybe_extract_knowledge(
        &self,
        observation: &Observation,
    ) -> Result<Option<KnowledgeInput>> {
        let dominated_by_generalizable = observation
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
Return JSON: {{"extract": true, "knowledge_type": "skill|pattern|gotcha|architecture|tool_usage", "title": "...", "description": "...", "instructions": "...", "triggers": [...]}}

If this is project-specific and not generalizable:
Return JSON: {{"extract": false, "reason": "..."}}"#,
            observation.title,
            observation.observation_type,
            concepts_str,
            observation.narrative.as_deref().unwrap_or(""),
            facts_str,
        );

        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![Message { role: "user".to_owned(), content: prompt }],
            response_format: ResponseFormat { format_type: "json_object".to_owned() },
        };

        let content = self.chat_completion(&request).await?;
        let stripped = opencode_mem_core::strip_markdown_json(&content);
        let extraction: KnowledgeExtractionResult =
            serde_json::from_str(stripped).map_err(|e| {
                Error::LlmApi(format!(
                    "Failed to parse extraction JSON: {e} - content: {}",
                    content.get(..300).unwrap_or(&content)
                ))
            })?;

        if !extraction.extract {
            return Ok(None);
        }

        let knowledge_type_str = extraction.knowledge_type.unwrap_or_else(|| "skill".to_owned());
        let knowledge_type = knowledge_type_str.parse::<KnowledgeType>().unwrap_or_else(|_| {
            tracing::warn!(
                invalid_type = %knowledge_type_str,
                "LLM returned unknown knowledge type, defaulting to Skill"
            );
            KnowledgeType::Skill
        });

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
