use std::sync::Arc;

use opencode_mem_core::{
    env_parse_with_default, filter_private_content, is_low_value_observation,
    observation_embedding_text, Observation, ObservationInput, ObservationType, ToolCall,
    ToolOutput,
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
    dedup_threshold: f32,
}

impl ObservationService {
    #[must_use]
    pub fn new(
        storage: Arc<StorageBackend>,
        llm: Arc<LlmClient>,
        infinite_mem: Option<Arc<InfiniteMemory>>,
        event_tx: broadcast::Sender<String>,
        embeddings: Option<Arc<EmbeddingService>>,
    ) -> Self {
        let dedup_threshold = env_parse_with_default("OPENCODE_MEM_DEDUP_THRESHOLD", 0.82_f32);
        Self { storage, llm, infinite_mem, event_tx, embeddings, dedup_threshold }
    }

    pub async fn process(
        &self,
        id: &str,
        tool_call: ToolCall,
    ) -> anyhow::Result<Option<Observation>> {
        if let Some((observation, was_new)) = self.compress_and_save(id, &tool_call).await? {
            // Only extract knowledge for genuinely new observations.
            // Merged observations already had knowledge extracted on first save.
            if was_new {
                self.extract_knowledge(&observation).await;
            }
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
    ) -> anyhow::Result<Option<(Observation, bool)>> {
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

        let existing_titles = self.find_existing_similar_titles(&tool_call.output).await;

        let observation = if let Some(obs) = self
            .llm
            .compress_to_observation(
                id,
                &input,
                tool_call.project.as_deref().filter(|p| !p.is_empty() && *p != "unknown"),
                &existing_titles,
            )
            .await?
        {
            obs
        } else {
            tracing::debug!("Observation filtered as trivial");
            return Ok(None);
        };

        let was_new = self.persist_and_notify(&observation).await?;

        Ok(Some((observation, was_new)))
    }

    async fn find_existing_similar_titles(&self, raw_text: &str) -> Vec<String> {
        let Some(emb_service) = self.embeddings.as_ref() else {
            return Vec::new();
        };

        let max_len = 2000;
        let end = if raw_text.len() <= max_len {
            raw_text.len()
        } else {
            let mut boundary = max_len;
            while boundary > 0 && !raw_text.is_char_boundary(boundary) {
                boundary = boundary.saturating_sub(1);
            }
            boundary
        };
        let truncated = raw_text.get(..end).unwrap_or("");
        let emb = Arc::clone(emb_service);
        let text = truncated.to_owned();
        let embed_result = tokio::task::spawn_blocking(move || emb.embed(&text)).await;

        let embedding = match embed_result {
            Ok(Ok(vec)) => vec,
            Ok(Err(e)) => {
                tracing::warn!("Failed to generate embedding for context search: {}", e);
                return Vec::new();
            },
            Err(e) => {
                tracing::warn!("Embedding task panicked for context search: {}", e);
                return Vec::new();
            },
        };

        match self.storage.find_similar_many(&embedding, 0.5, 10).await {
            Ok(matches) => {
                let titles: Vec<String> = matches.into_iter().map(|m| m.title).collect();
                if !titles.is_empty() {
                    tracing::debug!(
                        count = titles.len(),
                        "Found existing similar observations for dedup context"
                    );
                }
                titles
            },
            Err(e) => {
                tracing::warn!("find_similar_many failed for context: {}", e);
                Vec::new()
            },
        }
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
            let filtered_output = filter_private_content(&tool_call.output);
            let event = tool_event(
                &tool_call.session_id,
                tool_call.project.as_deref(),
                &tool_call.tool,
                tool_call.input.clone(),
                serde_json::json!({"output": filtered_output}),
                observation.files_modified.clone(),
            );
            if let Err(e) = infinite_mem.store_event(event).await {
                tracing::warn!(error = %e, observation_id = %observation.id, "Failed to store in infinite memory — event will be missing from long-term history");
            }
        }
    }

    /// Save a user-provided memory directly as an observation, without LLM compression.
    ///
    /// Returns `None` if the observation is filtered as low-value.
    ///
    /// # Errors
    /// Returns error if text is empty or persistence fails.
    pub async fn save_memory(
        &self,
        text: &str,
        title: Option<&str>,
        project: Option<&str>,
    ) -> anyhow::Result<Option<Observation>> {
        let text = text.trim();
        if text.is_empty() {
            anyhow::bail!("Text is required for save_memory");
        }

        let title = match title {
            Some(t) if !t.trim().is_empty() => t.trim().to_string(),
            _ => text.chars().take(50).collect(),
        };

        if is_low_value_observation(&title) {
            tracing::debug!("Filtered low-value save_memory: {}", title);
            return Ok(None);
        }

        let project = project.map(str::trim).filter(|p| !p.is_empty()).map(ToOwned::to_owned);

        let obs = Observation::builder(
            uuid::Uuid::new_v4().to_string(),
            "manual".to_owned(),
            ObservationType::Discovery,
            title,
        )
        .maybe_project(project)
        .narrative(text.to_owned())
        .build();

        self.save_observation(&obs).await?;
        Ok(Some(obs))
    }

    pub async fn save_observation(&self, observation: &Observation) -> anyhow::Result<()> {
        let _was_new = self.persist_and_notify(observation).await?;
        Ok(())
    }

    /// Returns `true` if a new observation was persisted, `false` if merged into existing or filtered.
    async fn persist_and_notify(&self, observation: &Observation) -> anyhow::Result<bool> {
        if is_low_value_observation(&observation.title) {
            tracing::debug!("Filtered low-value observation: {}", observation.title);
            return Ok(false);
        }

        let embedding_vec = self.generate_embedding(observation).await;

        if let Some(ref vec) = embedding_vec {
            if let Some(merged) = self.try_dedup_merge(observation, vec).await? {
                self.regenerate_embedding(&merged).await;
                return Ok(false);
            }
        }

        self.save_and_notify(observation, embedding_vec).await
    }

    /// Check for semantic duplicates and merge if found.
    /// Returns `Some(existing_id)` if merged, `None` if no match or merge failed.
    async fn try_dedup_merge(
        &self,
        observation: &Observation,
        embedding: &[f32],
    ) -> anyhow::Result<Option<String>> {
        if self.dedup_threshold <= 0.0 {
            return Ok(None);
        }

        let existing = match self.storage.find_similar(embedding, self.dedup_threshold).await {
            Ok(Some(m)) => m,
            Ok(None) => return Ok(None),
            Err(e) => {
                tracing::warn!("Semantic dedup search failed, proceeding without: {}", e);
                return Ok(None);
            },
        };

        tracing::info!(
            existing_id = %existing.observation_id,
            existing_title = %existing.title,
            new_title = %observation.title,
            similarity = %existing.similarity,
            "Semantic dedup: merging into existing observation"
        );

        match self.storage.merge_into_existing(&existing.observation_id, observation).await {
            Ok(()) => Ok(Some(existing.observation_id)),
            Err(e) => {
                tracing::warn!("Merge failed (target may have been deleted), saving as new: {}", e);
                Ok(None)
            },
        }
    }

    /// Re-generate and store the embedding for a merged observation.
    async fn regenerate_embedding(&self, observation_id: &str) {
        let merged_obs = match self.storage.get_by_id(observation_id).await {
            Ok(Some(obs)) => obs,
            Ok(None) => {
                tracing::warn!("Merged observation {} not found after merge", observation_id);
                return;
            },
            Err(e) => {
                tracing::warn!("Failed to re-read merged observation {}: {}", observation_id, e);
                return;
            },
        };

        if let Some(emb) = self.generate_embedding(&merged_obs).await {
            if let Err(e) = self.storage.store_embedding(observation_id, &emb).await {
                tracing::warn!("Failed to update embedding after merge: {}", e);
            }
        }
    }

    /// Insert a new observation into storage, send SSE notification, and store embedding.
    async fn save_and_notify(
        &self,
        observation: &Observation,
        embedding_vec: Option<Vec<f32>>,
    ) -> anyhow::Result<bool> {
        let inserted = self.storage.save_observation(observation).await?;
        if !inserted {
            tracing::debug!("Skipping duplicate observation: {}", observation.title);
            return Ok(false);
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

        Ok(true)
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
