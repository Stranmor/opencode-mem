use std::collections::HashSet;

use opencode_mem_core::{MAX_BATCH_IDS, NoiseLevel, cosine_similarity};
use opencode_mem_storage::traits::{EmbeddingStore, ObservationStore};

use super::ObservationService;
use crate::ServiceError;

struct ObservationSummary {
    id: String,
    noise_level: NoiseLevel,
    project: Option<String>,
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

        // Prepare combined data array for the blocking task
        let mut combined: Vec<(String, Vec<f32>, NoiseLevel, Option<String>)> =
            Vec::with_capacity(embeddings.len());
        for (id, emb) in embeddings {
            if let Some(summary) = summaries.iter().find(|s| s.id == id) {
                combined.push((id, emb, summary.noise_level, summary.project.clone()));
            }
        }

        let dedup_threshold = self.dedup_threshold;
        let combined_len = combined.len();
        let start_time = std::time::Instant::now();

        // Perform O(N^2) comparison in a blocking thread to avoid starving the async executor
        let merge_pairs = tokio::task::spawn_blocking(move || {
            let mut pairs = Vec::new();
            let mut deleted_ids = HashSet::new();

            for (i, (id_a, emb_a, noise_a, proj_a)) in combined.iter().enumerate() {
                if deleted_ids.contains(id_a) {
                    continue;
                }

                let start = i.saturating_add(1);
                for (id_b, emb_b, noise_b, proj_b) in combined.iter().skip(start) {
                    if deleted_ids.contains(id_b) {
                        continue;
                    }

                    // Critical: Prevent context starvation by only merging within the same project
                    if proj_a != proj_b {
                        continue;
                    }

                    let sim = cosine_similarity(emb_a, emb_b);
                    if sim < dedup_threshold {
                        continue;
                    }

                    // Lower NoiseLevel ord value = more important (Critical < High < Medium < Low < Negligible).
                    // Keep the more important observation (lower ord = keeper).
                    let (keeper_id, duplicate_id) =
                        if noise_a <= noise_b { (id_a, id_b) } else { (id_b, id_a) };

                    pairs.push((keeper_id.to_string(), duplicate_id.to_string(), sim));
                    deleted_ids.insert(duplicate_id.to_string());

                    if duplicate_id == id_a {
                        break;
                    }
                }
            }
            pairs
        })
        .await
        .map_err(|e| ServiceError::System(anyhow::anyhow!("spawn_blocking failed: {}", e)))?;

        let elapsed = start_time.elapsed();
        tracing::debug!(
            items = combined_len,
            elapsed_ms = elapsed.as_millis(),
            pairs_found = merge_pairs.len(),
            "Dedup sweep O(N^2) comparison phase completed"
        );

        let mut merged_count: usize = 0;
        for (keeper_id, duplicate_id, sim) in merge_pairs {
            if let Err(e) = self.merge_and_delete_duplicate(&keeper_id, &duplicate_id, sim).await {
                tracing::warn!(
                    keeper = %keeper_id,
                    duplicate = %duplicate_id,
                    error = %e,
                    "Dedup sweep: merge failed, skipping pair"
                );
                continue;
            }
            merged_count = merged_count.saturating_add(1);
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
            .map(|obs| ObservationSummary {
                id: obs.id,
                noise_level: obs.noise_level,
                project: obs.project,
            })
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
