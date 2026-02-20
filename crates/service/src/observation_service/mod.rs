mod dedup_sweep;
mod injection;
mod persistence;
mod side_effects;

use std::collections::HashSet;
use std::sync::Arc;

use opencode_mem_core::{
    Observation, ObservationInput, ObservationType, ToolCall, ToolOutput, env_parse_with_default,
    filter_injected_memory, is_trivial_tool_call,
};
use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_infinite::InfiniteMemory;
use opencode_mem_llm::{CompressionResult, LlmClient};
use opencode_mem_storage::StorageBackend;
use opencode_mem_storage::traits::{ObservationStore, SearchStore};
use tokio::sync::broadcast;

pub enum SaveMemoryResult {
    Created(Observation),
    Duplicate(Observation),
    Filtered,
}
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
        let raw_dedup = env_parse_with_default("OPENCODE_MEM_DEDUP_THRESHOLD", 0.85_f32);
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
        let save_result = self.compress_and_save(id, &tool_call).await?;

        // ALWAYS store raw event to infinite memory immediately, regardless of LLM compression result
        let observation_ref = save_result.as_ref().map(|(o, _)| o);
        let infinite_fut = self.store_infinite_memory(&tool_call, observation_ref);

        let extract_fut = async {
            if let Some((ref obs, _)) = save_result {
                self.extract_knowledge(obs).await;
            }
        };

        tokio::join!(extract_fut, infinite_fut);

        Ok(save_result.map(|(o, _)| o))
    }

    pub async fn compress_and_save(
        &self,
        id: &str,
        tool_call: &ToolCall,
    ) -> Result<Option<(Observation, bool)>, crate::ServiceError> {
        if is_trivial_tool_call(&tool_call.tool, &tool_call.input) {
            tracing::debug!(tool = %tool_call.tool, "Bypassing LLM compression for trivial tool call");
            return Ok(None);
        }

        // Apply centralized privacy and injection filtering exactly once here
        // before LLM compression or long-term persistence.
        let filtered_output =
            opencode_mem_core::filter_private_content(&filter_injected_memory(&tool_call.output));
        let filtered_input = {
            let input_str = serde_json::to_string(&tool_call.input).unwrap_or_default();
            let filtered =
                opencode_mem_core::filter_private_content(&filter_injected_memory(&input_str));
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

        let candidates =
            self.find_candidate_observations(&filtered_output, &tool_call.session_id).await;

        let compression_result = self
            .llm
            .compress_to_observation(
                id,
                &input,
                tool_call.project.as_deref().filter(|p| !p.is_empty() && *p != "unknown"),
                &candidates,
            )
            .await?;

        match compression_result {
            CompressionResult::Skip { reason } => {
                tracing::debug!(reason = %reason, "Observation skipped by LLM");
                Ok(None)
            },
            CompressionResult::Create(observation) => {
                self.persist_and_notify(&observation, Some(&tool_call.session_id)).await
            },
            CompressionResult::Update { target_id, observation } => {
                let candidate_ids: HashSet<&str> =
                    candidates.iter().map(|o| o.id.as_str()).collect();
                if !candidate_ids.contains(target_id.as_str()) {
                    tracing::warn!(
                        target_id = %target_id,
                        "Update target not in candidate set at service layer — treating as create"
                    );
                    return self
                        .persist_and_notify(&observation, Some(&tool_call.session_id))
                        .await;
                }

                match self.storage.merge_into_existing(&target_id, &observation).await {
                    Ok(()) => {
                        tracing::info!(
                            target_id = %target_id,
                            title = %observation.title,
                            "Context-aware update: merged into existing observation"
                        );
                        self.regenerate_embedding(&target_id).await;
                        let merged_obs = self.storage.get_by_id(&target_id).await?;
                        match merged_obs {
                            Some(obs) => Ok(Some((obs, false))),
                            None => {
                                tracing::warn!(
                                    target_id = %target_id,
                                    "Merged observation disappeared after merge, saving as new"
                                );
                                self.persist_and_notify(&observation, Some(&tool_call.session_id))
                                    .await
                            },
                        }
                    },
                    Err(e) => {
                        tracing::warn!(
                            target_id = %target_id,
                            error = %e,
                            "Merge failed, falling back to create"
                        );
                        self.persist_and_notify(&observation, Some(&tool_call.session_id)).await
                    },
                }
            },
        }
    }

    async fn find_candidate_observations(
        &self,
        raw_text: &str,
        session_id: &str,
    ) -> Vec<Observation> {
        // Phase 1: FTS search for top 5 candidates matching raw input
        let fts_candidates = self.find_fts_candidates(raw_text).await;

        // Phase 2: Get 3 most recent observations from same session
        let session_candidates = match self.storage.get_session_observations(session_id).await {
            Ok(obs) => {
                let len = obs.len();
                let start = len.saturating_sub(3);
                obs.into_iter().skip(start).collect()
            },
            Err(e) => {
                tracing::warn!(error = %e, "Failed to get session observations for candidates");
                Vec::new()
            },
        };

        // Phase 3: Union by id (deduplicate)
        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut result: Vec<Observation> = Vec::new();
        for obs in fts_candidates.into_iter().chain(session_candidates) {
            if seen_ids.insert(obs.id.clone()) {
                result.push(obs);
            }
        }

        if !result.is_empty() {
            tracing::debug!(
                count = result.len(),
                "Found candidate observations for context-aware compression"
            );
        }

        result
    }

    async fn find_fts_candidates(&self, raw_text: &str) -> Vec<Observation> {
        let max_query_len = 500;
        let end = if raw_text.len() <= max_query_len {
            raw_text.len()
        } else {
            let mut boundary = max_query_len;
            while boundary > 0 && !raw_text.is_char_boundary(boundary) {
                boundary = boundary.saturating_sub(1);
            }
            boundary
        };
        let query = raw_text.get(..end).unwrap_or("");
        if query.is_empty() {
            return Vec::new();
        }

        let search_results = match self.storage.search(query, 5).await {
            Ok(results) => results,
            Err(e) => {
                tracing::warn!(error = %e, "FTS search for candidates failed");
                return Vec::new();
            },
        };

        if search_results.is_empty() {
            return Vec::new();
        }

        let ids: Vec<String> = search_results.into_iter().map(|r| r.id).collect();
        match self.storage.get_observations_by_ids(&ids).await {
            Ok(obs) => obs,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch candidate observations by ids");
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
    ) -> Result<SaveMemoryResult, crate::ServiceError> {
        let raw_text = text.trim();
        let text = opencode_mem_core::filter_private_content(raw_text);
        let text = filter_injected_memory(&text);
        if text.is_empty() {
            return Err(crate::ServiceError::InvalidInput(
                "Text is required for save_memory".into(),
            ));
        }

        let title_str = match title {
            Some(t) if !t.trim().is_empty() => t.trim().to_string(),
            _ => text.chars().take(50).collect(),
        };

        let project_str = project.map(str::trim).filter(|p| !p.is_empty()).map(ToOwned::to_owned);

        let obs = Observation::builder(
            uuid::Uuid::new_v4().to_string(),
            "manual".to_owned(),
            ObservationType::Discovery,
            title_str,
        )
        .maybe_project(project_str.clone())
        .narrative(text.to_owned())
        .build();

        if let Some(ref infinite_mem) = self.infinite_mem {
            let event = opencode_mem_infinite::tool_event(
                "manual",
                project_str.as_deref(),
                "save_memory",
                serde_json::json!({"text": text}),
                serde_json::json!({"output": "saved manually"}),
                vec![],
            );
            if let Err(e) = infinite_mem.store_event(event).await {
                tracing::warn!(error = %e, "Failed to store manual save_memory event in infinite memory");
            }
        }

        let result = self.persist_and_notify(&obs, None).await?;
        match result {
            Some((persisted_obs, _was_new)) => Ok(SaveMemoryResult::Created(persisted_obs)),
            None => Ok(SaveMemoryResult::Duplicate(obs)), // If not filtered, must be duplicate
        }
    }
}
