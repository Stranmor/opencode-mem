use opencode_mem_core::{
    strip_markdown_json, Concept, Error, KnowledgeExtractionResult, KnowledgeInput, KnowledgeType,
    Observation, Result,
};

use crate::ai_types::{ChatRequest, ChatResponse, Message, ResponseFormat};
use crate::client::LlmClient;

impl LlmClient {
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
        let concepts_str = observation
            .concepts
            .iter()
            .map(|c| format!("{:?}", c))
            .collect::<Vec<_>>()
            .join(", ");

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
        let extraction: KnowledgeExtractionResult = serde_json::from_str(content).map_err(|e| {
            Error::LlmApi(format!(
                "Failed to parse extraction JSON: {} - content: {}",
                e,
                &content[..content.len().min(300)]
            ))
        })?;

        if !extraction.extract {
            return Ok(None);
        }

        let knowledge_type_str = extraction
            .knowledge_type
            .unwrap_or_else(|| "skill".to_string());
        let knowledge_type = knowledge_type_str
            .parse::<KnowledgeType>()
            .unwrap_or(KnowledgeType::Skill);

        Ok(Some(KnowledgeInput {
            knowledge_type,
            title: extraction
                .title
                .unwrap_or_else(|| observation.title.clone()),
            description: extraction
                .description
                .unwrap_or_else(|| observation.narrative.clone().unwrap_or_default()),
            instructions: extraction.instructions,
            triggers: extraction.triggers.unwrap_or_default(),
            source_project: observation.project.clone(),
            source_observation: Some(observation.id.clone()),
        }))
    }
}
