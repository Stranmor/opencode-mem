use opencode_mem_core::{Observation, is_low_value_observation};
use opencode_mem_storage::traits::{EmbeddingStore, ObservationStore};

use super::ObservationService;
use crate::ServiceError;

impl ObservationService {
    pub async fn save_observation(&self, observation: &Observation) -> Result<(), ServiceError> {
        let _result = self.persist_and_notify(observation, None).await?;
        Ok(())
    }

    /// Persist an observation, handling dedup merge and filtering.
    ///
    /// Returns:
    /// - `Ok(None)` — observation was filtered (low-value or echo of injected context)
    /// - `Ok(Some((obs, true)))` — newly inserted observation
    /// - `Ok(Some((obs, false)))` — merged into existing; returns the **existing** (merged) observation
    pub(crate) async fn persist_and_notify(
        &self,
        observation: &Observation,
        session_id: Option<&str>,
    ) -> Result<Option<(Observation, bool)>, ServiceError> {
        if is_low_value_observation(&observation.title) {
            tracing::debug!("Filtered low-value observation: {}", observation.title);
            return Ok(None);
        }

        let embedding_vec = self.generate_embedding(observation).await;

        if let Some(ref vec) = embedding_vec {
            if self.is_echo_of_injected(session_id, vec).await {
                tracing::info!(
                    title = %observation.title,
                    "Skipping observation — echo of injected context"
                );
                return Ok(None);
            }

            if let Some(merged_id) = self.try_dedup_merge(observation, vec).await? {
                self.regenerate_embedding(&merged_id).await;
                // Fetch the actual merged observation from storage so callers
                // get a real persisted entity, not a phantom with a temporary ID.
                let merged_obs = self.storage.get_by_id(&merged_id).await?;
                return match merged_obs {
                    Some(obs) => Ok(Some((obs, false))),
                    None => {
                        // Merged target was deleted between merge and fetch — shouldn't
                        // happen in practice, but handle gracefully by saving as new.
                        tracing::warn!(
                            merged_id = %merged_id,
                            "Merged observation disappeared after merge, saving as new"
                        );
                        self.save_and_notify(observation, embedding_vec).await
                    },
                };
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
    ) -> Result<Option<String>, ServiceError> {
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

    pub(crate) async fn regenerate_embedding(&self, observation_id: &str) {
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

    async fn save_and_notify(
        &self,
        observation: &Observation,
        embedding_vec: Option<Vec<f32>>,
    ) -> Result<Option<(Observation, bool)>, ServiceError> {
        let inserted = self.storage.save_observation(observation).await?;
        if !inserted {
            tracing::debug!("Skipping duplicate observation: {}", observation.title);
            return Ok(None);
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

        Ok(Some((observation.clone(), true)))
    }
}
