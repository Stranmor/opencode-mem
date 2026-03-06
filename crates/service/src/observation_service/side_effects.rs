use opencode_mem_core::{Observation, ToolCall, sanitize_input, tool_event};
use opencode_mem_storage::traits::KnowledgeStore;

use super::ObservationService;

impl ObservationService {
    pub(crate) async fn extract_knowledge(
        &self,
        observation: &Observation,
    ) -> Result<(), crate::ServiceError> {
        // Prevent duplicate extraction if knowledge is already present.
        match self
            .storage
            .has_knowledge_for_observation(observation.id.as_ref())
            .await
        {
            Ok(true) => {
                tracing::debug!(
                    "Knowledge already extracted for observation {}, skipping",
                    observation.id
                );
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to check existing knowledge for observation {}: {}",
                    observation.id,
                    e
                );
            }
            Ok(false) => {}
        }

        match self.llm.maybe_extract_knowledge(observation).await {
            Ok(Some(knowledge_input)) => match self.storage.save_knowledge(knowledge_input).await {
                Ok(knowledge) => {
                    tracing::info!(
                        "Auto-extracted knowledge: {} - {}",
                        knowledge.id,
                        knowledge.title
                    );
                    Ok(())
                }
                Err(e) => {
                    tracing::warn!("Failed to save extracted knowledge: {}", e);
                    Err(crate::ServiceError::Storage(e))
                }
            },
            Ok(None) => Ok(()),
            Err(e) => {
                tracing::warn!(error = %e, observation_id = %observation.id, "Knowledge extraction failed — knowledge will be missing for this observation");
                Err(crate::ServiceError::Llm(e))
            }
        }
    }

    pub(crate) async fn store_infinite_memory(
        &self,
        tool_call: &ToolCall,
        observation: Option<&Observation>,
    ) -> Result<(), crate::ServiceError> {
        if let Some(ref infinite_mem) = self.infinite_mem {
            let filtered_output = sanitize_input(&tool_call.output);
            let filtered_input = opencode_mem_core::sanitize_json_values(&tool_call.input);

            let files_modified = observation.map_or_else(Vec::new, |o| o.files_modified.clone());
            let obs_id_for_log =
                observation.map_or_else(|| tool_call.call_id.as_str(), |o| o.id.0.as_str());

            let event = tool_event(
                tool_call.session_id.as_ref(),
                tool_call.project.as_deref(),
                &tool_call.tool,
                filtered_input,
                serde_json::json!({"output": filtered_output}),
                files_modified,
                Some(tool_call.call_id.clone()),
            );
            if let Err(e) = infinite_mem.store_event(event).await {
                tracing::warn!(error = %e, observation_id = %obs_id_for_log, "Failed to store in infinite memory — event will be missing from long-term history");
                return Err(crate::ServiceError::System(e));
            }
        }
        Ok(())
    }
}
