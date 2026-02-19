use std::sync::Arc;

use opencode_mem_core::{cosine_similarity, observation_embedding_text, Observation};
use opencode_mem_embeddings::EmbeddingProvider;
use opencode_mem_storage::traits::{EmbeddingStore, InjectionStore};

use super::ObservationService;

const MAX_INJECTED_IDS: usize = 500;

impl ObservationService {
    pub(crate) async fn is_echo_of_injected(
        &self,
        session_id: Option<&str>,
        embedding: &[f32],
    ) -> bool {
        let session_id = match session_id {
            Some(id) if !id.is_empty() => id,
            _ => return false,
        };

        if self.injection_dedup_threshold <= 0.0 {
            return false;
        }

        let mut injected_ids = match self.storage.get_injected_observation_ids(session_id).await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!("Failed to get injected observation IDs: {}", e);
                return false;
            },
        };

        if injected_ids.is_empty() {
            return false;
        }

        if injected_ids.len() > MAX_INJECTED_IDS {
            tracing::warn!(session_id = %session_id, count = injected_ids.len(), cap = MAX_INJECTED_IDS, "Truncating injected observation IDs to cap (most recent)");
            let start = injected_ids.len().saturating_sub(MAX_INJECTED_IDS);
            injected_ids = injected_ids.split_off(start);
        }

        let injected_embeddings = match self.storage.get_embeddings_for_ids(&injected_ids).await {
            Ok(embs) => embs,
            Err(e) => {
                tracing::warn!("Failed to get embeddings for injected observations: {}", e);
                return false;
            },
        };

        for (obs_id, inj_emb) in &injected_embeddings {
            let sim = cosine_similarity(embedding, inj_emb);
            if sim >= self.injection_dedup_threshold {
                tracing::info!(
                    injected_id = %obs_id,
                    similarity = %sim,
                    threshold = %self.injection_dedup_threshold,
                    "Echo detected: new observation matches injected context"
                );
                return true;
            }
        }

        false
    }

    pub async fn cleanup_old_injections(&self) -> anyhow::Result<u64> {
        self.storage.cleanup_old_injections(24).await.map_err(Into::into)
    }

    /// Record which observation IDs were injected into a session for echo detection.
    pub async fn save_injected_observations(
        &self,
        session_id: &str,
        observation_ids: &[String],
    ) -> anyhow::Result<()> {
        self.storage.save_injected_observations(session_id, observation_ids).await.map_err(Into::into)
    }

    pub(crate) async fn generate_embedding(&self, observation: &Observation) -> Option<Vec<f32>> {
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
