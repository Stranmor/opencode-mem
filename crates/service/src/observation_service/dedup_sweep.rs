use std::collections::HashSet;

use opencode_mem_core::{cosine_similarity, NoiseLevel, MAX_BATCH_IDS};
use opencode_mem_storage::traits::{EmbeddingStore, ObservationStore};

use super::ObservationService;
use crate::ServiceError;

struct ObservationSummary {
    id: String,
    noise_level: NoiseLevel,
}

impl ObservationService {
    /// Periodic background sweep that finds and merges semantically duplicate observations.
    ///
    /// Loads all observations and their embeddings, compares pairs via cosine similarity,
    /// and merges duplicates above the dedup threshold (0.85).
    pub async fn run_dedup_sweep(&self) -> Result<usize, ServiceError> {
        if self.dedup_threshold <= 0.0 {
            return Ok(0);
        }

        let summaries = self.load_observation_summaries().await?;
        if summaries.is_empty() {
            return Ok(0);
        }

        let ids: Vec<String> = summaries.iter().map(|s| s.id.clone()).collect();
        let embeddings = self.load_all_embeddings(&ids).await?;

        if embeddings.is_empty() {
            return Ok(0);
        }

        let mut merged_count: usize = 0;
        let mut deleted_ids: HashSet<String> = HashSet::new();

        for (i, (id_a, emb_a)) in embeddings.iter().enumerate() {
            if deleted_ids.contains(id_a) {
                continue;
            }

            let start = i.saturating_add(1);
            for (id_b, emb_b) in embeddings.iter().skip(start) {
                if deleted_ids.contains(id_b) {
                    continue;
                }

                let sim = cosine_similarity(emb_a, emb_b);
                if sim < self.dedup_threshold {
                    continue;
                }

                let summary_a = summaries.iter().find(|s| s.id == *id_a);
                let summary_b = summaries.iter().find(|s| s.id == *id_b);

                let (Some(sa), Some(sb)) = (summary_a, summary_b) else {
                    continue;
                };

                // Lower NoiseLevel ord value = more important (Critical < High < Medium < Low < Negligible).
                // Keep the more important observation (lower ord = keeper).
                let (keeper_id, duplicate_id) = if sa.noise_level <= sb.noise_level {
                    (&sa.id, &sb.id)
                } else {
                    (&sb.id, &sa.id)
                };

                if let Err(e) = self.merge_and_delete_duplicate(keeper_id, duplicate_id, sim).await
                {
                    tracing::warn!(
                        keeper = %keeper_id,
                        duplicate = %duplicate_id,
                        error = %e,
                        "Dedup sweep: merge failed, skipping pair"
                    );
                    continue;
                }

                deleted_ids.insert(duplicate_id.clone());
                merged_count = merged_count.saturating_add(1);
            }
        }

        if merged_count > 0 {
            tracing::info!(
                merged = merged_count,
                "Dedup sweep completed: merged and removed duplicate observations"
            );
        } else {
            tracing::debug!("Dedup sweep completed: no duplicates found");
        }

        Ok(merged_count)
    }

    async fn load_observation_summaries(&self) -> Result<Vec<ObservationSummary>, ServiceError> {
        let observations = self.storage.get_recent(MAX_BATCH_IDS).await?;
        Ok(observations
            .into_iter()
            .map(|obs| ObservationSummary { id: obs.id, noise_level: obs.noise_level })
            .collect())
    }

    async fn load_all_embeddings(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Vec<f32>)>, ServiceError> {
        let mut all_embeddings = Vec::new();
        for chunk in ids.chunks(MAX_BATCH_IDS) {
            let batch = self.storage.get_embeddings_for_ids(chunk).await?;
            all_embeddings.extend(batch);
        }
        Ok(all_embeddings)
    }

    async fn merge_and_delete_duplicate(
        &self,
        keeper_id: &str,
        duplicate_id: &str,
        similarity: f32,
    ) -> Result<(), ServiceError> {
        let duplicate_obs = self.storage.get_by_id(duplicate_id).await?;
        let Some(dup) = duplicate_obs else {
            return Ok(());
        };

        tracing::info!(
            keeper = %keeper_id,
            duplicate = %duplicate_id,
            duplicate_title = %dup.title,
            similarity = %similarity,
            "Dedup sweep: merging duplicate into keeper"
        );

        self.storage.merge_into_existing(keeper_id, &dup).await?;

        self.regenerate_embedding(keeper_id).await;

        if self.storage.delete_observation_by_id(duplicate_id).await? {
            tracing::debug!(id = %duplicate_id, "Dedup sweep: deleted duplicate observation");
        }

        Ok(())
    }
}
