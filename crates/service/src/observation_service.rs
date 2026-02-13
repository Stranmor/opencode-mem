use std::sync::Arc;

use opencode_mem_core::{
    env_parse_with_default, is_low_value_observation, observation_embedding_text, Observation,
    ObservationInput, ToolCall, ToolOutput,
};
use opencode_mem_embeddings::{EmbeddingProvider, EmbeddingService};
use opencode_mem_infinite::{tool_event, InfiniteMemory};
use opencode_mem_llm::LlmClient;
use opencode_mem_storage::traits::{EmbeddingStore, KnowledgeStore, ObservationStore};
use opencode_mem_storage::StorageBackend;
use tokio::sync::broadcast;

pub struct ObservationService {
    storage: Arc<StorageBackend>,
    llm: Arc<LlmClient>,
    infinite_mem: Option<Arc<InfiniteMemory>>,
    event_tx: broadcast::Sender<String>,
    embeddings: Option<Arc<EmbeddingService>>,
}

impl ObservationService {
    #[must_use]
    pub const fn new(
        storage: Arc<StorageBackend>,
        llm: Arc<LlmClient>,
        infinite_mem: Option<Arc<InfiniteMemory>>,
        event_tx: broadcast::Sender<String>,
        embeddings: Option<Arc<EmbeddingService>>,
    ) -> Self {
        Self { storage, llm, infinite_mem, event_tx, embeddings }
    }

    pub async fn process(
        &self,
        id: &str,
        tool_call: ToolCall,
    ) -> anyhow::Result<Option<Observation>> {
        if let Some(observation) = self.compress_and_save(id, &tool_call).await? {
            self.extract_knowledge(&observation).await;
            self.store_infinite_memory(&tool_call, &observation).await;
            Ok(Some(observation))
        } else {
            Ok(None)
        }
    }

    pub async fn compress_and_save(
        &self,
        id: &str,
        tool_call: &ToolCall,
    ) -> anyhow::Result<Option<Observation>> {
        let input = ObservationInput::new(
            tool_call.tool.clone(),
            tool_call.session_id.clone(),
            tool_call.call_id.clone(),
            ToolOutput::new(
                format!("Observation from {}", tool_call.tool),
                tool_call.output.clone(),
                tool_call.input.clone(),
            ),
        );
        let observation = if let Some(obs) = self
            .llm
            .compress_to_observation(
                id,
                &input,
                tool_call.project.as_deref().filter(|p| !p.is_empty() && *p != "unknown"),
            )
            .await?
        {
            obs
        } else {
            tracing::debug!("Observation filtered as trivial");
            return Ok(None);
        };

        self.persist_and_notify(&observation).await?;

        Ok(Some(observation))
    }

    pub async fn extract_knowledge(&self, observation: &Observation) {
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

    async fn store_infinite_memory(&self, tool_call: &ToolCall, observation: &Observation) {
        if let Some(ref infinite_mem) = self.infinite_mem {
            let event = tool_event(
                &tool_call.session_id,
                tool_call.project.as_deref(),
                &tool_call.tool,
                tool_call.input.clone(),
                serde_json::json!({"output": tool_call.output}),
                observation.files_modified.clone(),
            );
            if let Err(e) = infinite_mem.store_event(event).await {
                tracing::warn!(error = %e, observation_id = %observation.id, "Failed to store in infinite memory — event will be missing from long-term history");
            }
        }
    }

    pub async fn save_observation(&self, observation: &Observation) -> anyhow::Result<()> {
        self.persist_and_notify(observation).await
    }

    async fn persist_and_notify(&self, observation: &Observation) -> anyhow::Result<()> {
        if is_low_value_observation(&observation.title) {
            tracing::debug!("Filtered low-value observation: {}", observation.title);
            return Ok(());
        }

        let embedding_vec = self.generate_embedding(observation).await;

        if let Some(ref vec) = embedding_vec {
            let threshold: f32 = env_parse_with_default("OPENCODE_MEM_DEDUP_THRESHOLD", 0.92_f32);
            if threshold > 0.0 {
                match self.storage.find_similar(vec, threshold).await {
                    Ok(Some(existing)) => {
                        tracing::info!(
                            existing_id = %existing.observation_id,
                            existing_title = %existing.title,
                            new_title = %observation.title,
                            similarity = %existing.similarity,
                            "Semantic dedup: merging into existing observation"
                        );
                        match self
                            .storage
                            .merge_into_existing(&existing.observation_id, observation)
                            .await
                        {
                            Ok(()) => {
                                if let Err(e) = self
                                    .storage
                                    .store_embedding(&existing.observation_id, vec)
                                    .await
                                {
                                    tracing::warn!("Failed to update embedding after merge: {}", e);
                                }
                                return Ok(());
                            },
                            Err(e) => {
                                tracing::warn!(
                                    "Merge failed (target may have been deleted), saving as new: {}",
                                    e
                                );
                            },
                        }
                    },
                    Ok(None) => {},
                    Err(e) => {
                        tracing::warn!("Semantic dedup search failed, proceeding without: {}", e);
                    },
                }
            }
        }

        let inserted = self.storage.save_observation(observation).await?;
        if !inserted {
            tracing::debug!("Skipping duplicate observation: {}", observation.title);
            return Ok(());
        }

        tracing::info!("Saved observation: {} - {}", observation.id, observation.title);
        if self.event_tx.send(serde_json::to_string(observation)?).is_err() {
            tracing::debug!("No SSE subscribers for observation event (this is normal at startup)");
        }

        if let Some(vec) = embedding_vec {
            match self.storage.store_embedding(&observation.id, &vec).await {
                Ok(()) => {
                    tracing::info!("Stored embedding for {}", observation.id);
                },
                Err(e) => {
                    tracing::warn!("Failed to store embedding for {}: {}", observation.id, e);
                },
            }
        }

        Ok(())
    }

    async fn generate_embedding(&self, observation: &Observation) -> Option<Vec<f32>> {
        let emb = self.embeddings.as_ref()?;
        let text = observation_embedding_text(observation);
        let emb = Arc::clone(emb);
        let result = tokio::task::spawn_blocking(move || emb.embed(&text)).await;
        match result {
            Ok(Ok(vec)) => Some(vec),
            Ok(Err(e)) => {
                tracing::warn!("Failed to generate embedding for {}: {}", observation.id, e);
                None
            },
            Err(e) => {
                tracing::warn!("Embedding task panicked for {}: {}", observation.id, e);
                None
            },
        }
    }
}
