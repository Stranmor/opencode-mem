use opencode_mem_core::{is_low_value_observation, Observation};
use opencode_mem_storage::traits::{EmbeddingStore, ObservationStore};

use super::ObservationService;

impl ObservationService {
    pub async fn save_observation(&self, observation: &Observation) -> anyhow::Result<()> {
        let _was_new = self.persist_and_notify(observation, None).await?;
        Ok(())
    }

    /// Returns `true` if a new observation was persisted, `false` if merged into existing or filtered.
    pub(crate) async fn persist_and_notify(
        &self,
        observation: &Observation,
        session_id: Option<&str>,
    ) -> anyhow::Result<bool> {
        if is_low_value_observation(&observation.title) {
            tracing::debug!("Filtered low-value observation: {}", observation.title);
            return Ok(false);
        }

        let embedding_vec = self.generate_embedding(observation).await;

        if let Some(ref vec) = embedding_vec {
            if self.is_echo_of_injected(session_id, vec).await {
                tracing::info!(
                    title = %observation.title,
                    "Skipping observation — echo of injected context"
                );
                return Ok(false);
            }

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
}
