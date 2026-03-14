mod compression;
mod dedup_sweep;
mod injection;
mod persistence;
mod save_memory;
mod side_effects;

use std::sync::Arc;

use opencode_mem_core::{AppConfig, Observation, ToolCall};
use opencode_mem_embeddings::LazyEmbeddingService;
use opencode_mem_llm::LlmClient;
use opencode_mem_storage::StorageBackend;
use opencode_mem_storage::traits::ObservationStore;
use tokio::sync::{Semaphore, broadcast};

use crate::InfiniteMemoryService;

pub enum SaveMemoryResult {
    Created(Observation),
    Duplicate(Observation),
    Filtered,
}
#[derive(Clone)]
pub struct ObservationService {
    pub(crate) storage: Arc<StorageBackend>,
    pub(crate) llm: Arc<LlmClient>,
    pub(crate) infinite_mem: Option<Arc<InfiniteMemoryService>>,
    pub(crate) event_tx: broadcast::Sender<String>,
    pub(crate) embeddings: Option<Arc<LazyEmbeddingService>>,
    pub(crate) dedup_threshold: f32,
    pub(crate) injection_dedup_threshold: f32,
    pub(crate) project_filter: Option<opencode_mem_core::ProjectFilter>,
    pub(crate) low_value_filter: opencode_mem_core::LowValueFilter,
    pub(crate) enrichment_semaphore: Arc<Semaphore>,
}

impl ObservationService {
    pub fn circuit_breaker(&self) -> &opencode_mem_storage::CircuitBreaker {
        self.storage.circuit_breaker()
    }

    pub(crate) fn with_cb<T>(
        &self,
        result: Result<T, opencode_mem_storage::StorageError>,
    ) -> Result<T, crate::ServiceError> {
        result.map_err(crate::ServiceError::from)
    }

    pub fn update_llm_config(
        &self,
        api_key: Option<String>,
        base_url: Option<String>,
        model: Option<String>,
    ) {
        self.llm.update_config(api_key, base_url, model);
    }

    #[must_use]
    pub fn new(
        storage: Arc<StorageBackend>,
        llm: Arc<LlmClient>,
        infinite_mem: Option<Arc<InfiniteMemoryService>>,
        event_tx: broadcast::Sender<String>,
        embeddings: Option<Arc<LazyEmbeddingService>>,
        config: &AppConfig,
    ) -> Self {
        let dedup_threshold = config.dedup_threshold;
        let injection_dedup_threshold = config.injection_dedup_threshold;
        let project_filter =
            opencode_mem_core::ProjectFilter::new(config.excluded_projects_raw.as_deref());
        let low_value_filter =
            opencode_mem_core::LowValueFilter::new(config.filter_patterns_raw.as_deref());
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
            project_filter,
            low_value_filter,
            enrichment_semaphore: Arc::new(Semaphore::new(3)),
        }
    }

    pub async fn process(
        &self,
        id: &str,
        tool_call: ToolCall,
    ) -> Result<Option<Observation>, crate::ServiceError> {
        let result = self.storage.guarded(|| self.storage.get_by_id(id)).await;
        let existing_obs = self.with_cb(result)?;

        if let Some(obs) = existing_obs {
            tracing::info!(id = %id, "Observation already exists in primary storage, skipping LLM compression for queue retry");

            let infinite_fut = self.store_infinite_memory(&tool_call, Some(&obs));
            let extract_fut = self.extract_knowledge(&obs);

            let (extract_res, infinite_res) = tokio::join!(extract_fut, infinite_fut);
            extract_res?;
            // Gracefully handle infinite memory failures to avoid poisoning primary pipeline
            if let Err(e) = infinite_res {
                tracing::warn!(error = %e, "Failed to store existing observation in infinite memory (ignoring)");
            }

            return Ok(Some(obs));
        }

        let infinite_fut = self.store_infinite_memory(&tool_call, None);
        let compress_fut = async {
            let result = self.compress_and_save(id, &tool_call).await?;
            if let Some((ref obs, false)) = result
                && obs.id.0 != id
            {
                let tombstone = Observation::builder(
                    id.to_owned(),
                    obs.session_id.clone(),
                    obs.observation_type,
                    format!("[merged into {}]", obs.id),
                )
                .noise_level(opencode_mem_core::NoiseLevel::Negligible)
                .noise_reason(format!("Tombstone: merged into {}", obs.id))
                .build();

                let result = self
                    .storage
                    .guarded(|| self.storage.save_observation(&tombstone))
                    .await;
                if let Err(e) = self.with_cb(result) {
                    tracing::warn!(
                        tombstone_id = %id,
                        merged_into = %obs.id,
                        error = %e,
                        "Failed to save tombstone observation — stale original may remain visible"
                    );
                }
            }
            Ok::<_, crate::ServiceError>(result)
        };

        let (infinite_res, compress_res) = tokio::join!(infinite_fut, compress_fut);

        let save_result = compress_res?;

        // Gracefully handle infinite memory failures to avoid poisoning primary pipeline
        if let Err(e) = infinite_res {
            tracing::warn!(error = %e, "Failed to store observation in infinite memory (ignoring)");
        }

        if let Some((ref obs, _)) = save_result {
            self.extract_knowledge(obs).await?;
        }

        Ok(save_result.map(|(o, _)| o))
    }
}

#[cfg(test)]
mod adversarial_tests;
#[cfg(test)]
mod privacy_tests;

#[cfg(test)]
mod breaker_tests;
