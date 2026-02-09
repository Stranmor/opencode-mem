use std::sync::Arc;

use opencode_mem_core::{
    is_low_value_observation, Observation, ObservationInput, ToolCall, ToolOutput,
};
use opencode_mem_embeddings::{EmbeddingProvider, EmbeddingService};
use opencode_mem_infinite::{tool_event, InfiniteMemory};
use opencode_mem_llm::LlmClient;
use opencode_mem_storage::Storage;
use tokio::sync::broadcast;

pub struct ObservationService {
    storage: Arc<Storage>,
    llm: Arc<LlmClient>,
    infinite_mem: Option<Arc<InfiniteMemory>>,
    event_tx: broadcast::Sender<String>,
    embeddings: Option<Arc<EmbeddingService>>,
}

impl ObservationService {
    #[must_use]
    pub const fn new(
        storage: Arc<Storage>,
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
            Ok(Some(knowledge_input)) => {
                let storage = Arc::clone(&self.storage);
                let result =
                    tokio::task::spawn_blocking(move || storage.save_knowledge(knowledge_input))
                        .await;
                match result {
                    Ok(Ok(knowledge)) => {
                        tracing::info!(
                            "Auto-extracted knowledge: {} - {}",
                            knowledge.id,
                            knowledge.title
                        );
                    },
                    Ok(Err(e)) => {
                        tracing::warn!("Failed to save extracted knowledge: {}", e);
                    },
                    Err(e) => {
                        tracing::error!("Blocking task failed: {}", e);
                    },
                }
            },
            Ok(None) => {},
            Err(e) => {
                tracing::debug!("Knowledge extraction skipped: {}", e);
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
                tracing::warn!("Failed to store in infinite memory: {}", e);
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

        let storage = Arc::clone(&self.storage);
        let obs = observation.clone();
        tokio::task::spawn_blocking(move || storage.save_observation(&obs)).await??;

        tracing::info!("Saved observation: {} - {}", observation.id, observation.title);
        if self.event_tx.send(serde_json::to_string(observation)?).is_err() {
            tracing::debug!("No SSE subscribers for observation event (this is normal at startup)");
        }

        if let Some(ref emb) = self.embeddings {
            let text = format!(
                "{} {} {}",
                observation.title,
                observation.narrative.as_deref().unwrap_or(""),
                observation.facts.join(" ")
            );

            let emb = Arc::clone(emb);
            let embedding = tokio::task::spawn_blocking(move || emb.embed(&text)).await?;

            match embedding {
                Ok(vec) => {
                    let storage = Arc::clone(&self.storage);
                    let obs_id = observation.id.clone();
                    let result =
                        tokio::task::spawn_blocking(move || storage.store_embedding(&obs_id, &vec))
                            .await;

                    match result {
                        Ok(Ok(())) => {
                            tracing::info!("Stored embedding for {}", observation.id);
                        },
                        Ok(Err(e)) => {
                            tracing::warn!(
                                "Failed to store embedding for {}: {}",
                                observation.id,
                                e
                            );
                        },
                        Err(e) => {
                            tracing::error!("Blocking task failed: {}", e);
                        },
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to generate embedding for {}: {}", observation.id, e);
                },
            }
        }

        Ok(())
    }
}
