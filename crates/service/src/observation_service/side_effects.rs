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
            // Impose strict ceiling trims to prevent JSONB bloat and OOM in cron aggregation.
            // Truncate multi-megabyte tool outputs (like cat or multi-file grep) to a manageable size.
            const MAX_INFINITE_FIELD_LEN: usize = 10000;
            let sanitized_output = sanitize_input(&tool_call.output);
            let filtered_output =
                opencode_mem_core::truncate(&sanitized_output, MAX_INFINITE_FIELD_LEN);

            let mut filtered_input = tool_call.input.clone();
            opencode_mem_core::sanitize_json_values(&mut filtered_input);

            // Enforce input size guards to prevent memory explosion/DB bloat.
            // If the total serialized JSON exceeds 50KB, truncate it.
            let mut input_str = serde_json::to_string(&filtered_input).unwrap_or_default();
            if input_str.len() > 50000 {
                input_str = format!(
                    "{}\n... (truncated due to large size)",
                    opencode_mem_core::truncate(&input_str, 50000)
                );
                filtered_input = serde_json::Value::String(input_str);
            }

            let files_modified = observation.map_or_else(Vec::new, |o| o.files_modified.clone());
            let obs_id_for_log =
                observation.map_or_else(|| tool_call.call_id.as_str(), |o| o.id.0.as_str());

            let event = tool_event(
                tool_call.session_id.as_ref(),
                tool_call.project.as_deref(),
                &tool_call.tool,
                filtered_input,
                serde_json::json!({ "output": filtered_output }),
                files_modified,
                Some(tool_call.call_id.clone()),
            );
            if let Err(e) = infinite_mem.store_event(event).await {
                tracing::warn!(error = %e, observation_id = %obs_id_for_log, "Failed to store in infinite memory — event will be missing from long-term history");
                return Err(crate::ServiceError::System(e.into()));
            }
        }
        Ok(())
    }
}
