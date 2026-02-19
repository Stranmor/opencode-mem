mod injection;
mod persistence;
mod side_effects;

use std::sync::Arc;

use opencode_mem_core::{
    env_parse_with_default, filter_injected_memory, is_low_value_observation, Observation,
    ObservationInput, ObservationType, ToolCall, ToolOutput,
};
use opencode_mem_embeddings::{EmbeddingProvider, EmbeddingService};
use opencode_mem_infinite::InfiniteMemory;
use opencode_mem_llm::LlmClient;
use opencode_mem_storage::traits::EmbeddingStore;
use opencode_mem_storage::StorageBackend;
use tokio::sync::broadcast;

pub struct ObservationService {
    pub(crate) storage: Arc<StorageBackend>,
    pub(crate) llm: Arc<LlmClient>,
    pub(crate) infinite_mem: Option<Arc<InfiniteMemory>>,
    pub(crate) event_tx: broadcast::Sender<String>,
    pub(crate) embeddings: Option<Arc<EmbeddingService>>,
    pub(crate) dedup_threshold: f32,
    pub(crate) injection_dedup_threshold: f32,
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
        let raw_dedup = env_parse_with_default("OPENCODE_MEM_DEDUP_THRESHOLD", 0.92_f32);
        let dedup_threshold = raw_dedup.clamp(0.0, 1.0);
        if (dedup_threshold - raw_dedup).abs() > f32::EPSILON {
            tracing::warn!(
                original = raw_dedup,
                clamped = dedup_threshold,
                "OPENCODE_MEM_DEDUP_THRESHOLD clamped to [0.0, 1.0]"
            );
        }
        let raw_injection =
            env_parse_with_default("OPENCODE_MEM_INJECTION_DEDUP_THRESHOLD", 0.80_f32);
        let injection_dedup_threshold = raw_injection.clamp(0.0, 1.0);
        if (injection_dedup_threshold - raw_injection).abs() > f32::EPSILON {
            tracing::warn!(
                original = raw_injection,
                clamped = injection_dedup_threshold,
                "OPENCODE_MEM_INJECTION_DEDUP_THRESHOLD clamped to [0.0, 1.0]"
            );
        }
        if injection_dedup_threshold > 0.0 && embeddings.is_none() {
            tracing::warn!(
                threshold = %injection_dedup_threshold,
                "Injection dedup threshold is configured but embeddings are disabled — echo detection will not function"
            );
        }
        Self {
            storage,
            llm,
            infinite_mem,
            event_tx,
            embeddings,
            dedup_threshold,
            injection_dedup_threshold,
        }
    }

    pub async fn process(
        &self,
        id: &str,
        tool_call: ToolCall,
    ) -> Result<Option<Observation>, crate::ServiceError> {
        if let Some((observation, was_new)) = self.compress_and_save(id, &tool_call).await? {
            // Only extract knowledge for genuinely new observations.
            // Merged observations already had knowledge extracted on first save.
            let extract_fut = async {
                if was_new {
                    self.extract_knowledge(&observation).await;
                }
            };
            let infinite_fut = self.store_infinite_memory(&tool_call, &observation);
            tokio::join!(extract_fut, infinite_fut);
            Ok(Some(observation))
        } else {
            Ok(None)
        }
    }

    pub async fn compress_and_save(
        &self,
        id: &str,
        tool_call: &ToolCall,
    ) -> Result<Option<(Observation, bool)>, crate::ServiceError> {
        // Strip injected memory blocks to prevent recursion:
        // IDE injects <memory-global>...</memory-global> into conversation →
        // observe hook captures tool output containing these blocks →
        // without filtering, they'd be re-processed and re-stored → infinite loop.
        let filtered_output = filter_injected_memory(&tool_call.output);
        let filtered_input = {
            let input_str = serde_json::to_string(&tool_call.input).unwrap_or_default();
            let filtered = filter_injected_memory(&input_str);
            serde_json::from_str(&filtered).unwrap_or_else(|e| {
                tracing::warn!(
                    error = %e,
                    "Privacy/injection filter corrupted JSON input — using Null instead of unfiltered fallback"
                );
                serde_json::Value::Null
            })
        };

        let input = ObservationInput::new(
            tool_call.tool.clone(),
            tool_call.session_id.clone(),
            tool_call.call_id.clone(),
            ToolOutput::new(
                format!("Observation from {}", tool_call.tool),
                filtered_output.clone(),
                filtered_input,
            ),
        );

        let existing_titles = self.find_existing_similar_titles(&filtered_output).await;

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

        let result = self.persist_and_notify(&observation, Some(&tool_call.session_id)).await?;

        match result {
            Some((persisted_obs, was_new)) => Ok(Some((persisted_obs, was_new))),
            None => Ok(None),
        }
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
    ) -> Result<Option<Observation>, crate::ServiceError> {
        let text = text.trim();
        let text = &opencode_mem_core::filter_private_content(text);
        let text = &filter_injected_memory(text);
        if text.is_empty() {
            return Err(crate::ServiceError::InvalidInput(
                "Text is required for save_memory".into(),
            ));
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

        let result = self.persist_and_notify(&obs, None).await?;
        Ok(result.map(|(persisted_obs, _was_new)| persisted_obs))
    }
}
