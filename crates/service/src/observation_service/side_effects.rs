use opencode_mem_core::{Observation, ToolCall, filter_injected_memory, filter_private_content};
use opencode_mem_infinite::tool_event;
use opencode_mem_storage::traits::KnowledgeStore;

use super::ObservationService;

impl ObservationService {
    pub(crate) async fn extract_knowledge(&self, observation: &Observation) {
        match self.llm.maybe_extract_knowledge(observation).await {
            Ok(Some(knowledge_input)) => match self.storage.save_knowledge(knowledge_input).await {
                Ok(knowledge) => {
                    tracing::info!(
                        "Auto-extracted knowledge: {} - {}",
                        knowledge.id,
                        knowledge.title
                    );
                },
                Err(e) => {
                    tracing::warn!("Failed to save extracted knowledge: {}", e);
                },
            },
            Ok(None) => {},
            Err(e) => {
                tracing::warn!(error = %e, observation_id = %observation.id, "Knowledge extraction failed — knowledge will be missing for this observation");
            },
        }
    }

    pub(crate) async fn store_infinite_memory(
        &self,
        tool_call: &ToolCall,
        observation: Option<&Observation>,
    ) {
        if let Some(ref infinite_mem) = self.infinite_mem {
            let filtered_output =
                filter_injected_memory(&filter_private_content(&tool_call.output));
            let filtered_input = {
                let input_str = serde_json::to_string(&tool_call.input).unwrap_or_default();
                let filtered = filter_injected_memory(&filter_private_content(&input_str));
                serde_json::from_str(&filtered).unwrap_or_else(|e| {
                    tracing::warn!(
                        error = %e,
                        "Privacy/injection filter corrupted JSON input in infinite memory — using Null instead of unfiltered fallback"
                    );
                    serde_json::Value::Null
                })
            };

            let files_modified = observation.map_or_else(Vec::new, |o| o.files_modified.clone());
            let obs_id_for_log =
                observation.map_or_else(|| tool_call.call_id.as_str(), |o| o.id.as_str());

            let event = tool_event(
                &tool_call.session_id,
                tool_call.project.as_deref(),
                &tool_call.tool,
                filtered_input,
                serde_json::json!({"output": filtered_output}),
                files_modified,
            );
            if let Err(e) = infinite_mem.store_event(event).await {
                tracing::warn!(error = %e, observation_id = %obs_id_for_log, "Failed to store in infinite memory — event will be missing from long-term history");
            }
        }
    }
}
