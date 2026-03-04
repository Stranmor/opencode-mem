//! Embedding backfill and semantic deduplication logic for SearchService.

use std::collections::HashMap;
use std::sync::Arc;

use opencode_mem_core::{Observation, cosine_similarity};
use opencode_mem_embeddings::EmbeddingProvider;
use opencode_mem_storage::traits::EmbeddingStore;

use crate::ServiceError;

use super::SearchService;

/// Cosine similarity threshold above which two observations are considered semantic duplicates.
const DEDUP_SIMILARITY_THRESHOLD: f32 = 0.85;

impl SearchService {
    pub async fn clear_embeddings(&self) -> Result<(), ServiceError> {
        let result = self
            .storage
            .clear_embeddings()
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    #[allow(
        clippy::arithmetic_side_effects,
        reason = "total counter increment is safe - max value is batch_size iterations"
    )]
    /// Runs embedding backfill for a batch of observations
    pub async fn run_embedding_backfill(&self, batch_size: usize) -> Result<usize, ServiceError> {
        let Some(ref embeddings) = self.embeddings else {
            return Ok(0);
        };
        let mut total = 0;
        let mut failed_ids = std::collections::HashSet::new();
        loop {
            let all_obs = self
                .storage
                .get_observations_without_embeddings(batch_size)
                .await?;
            if all_obs.is_empty() {
                break;
            }
            let obs: Vec<_> = all_obs
                .into_iter()
                .filter(|o| !failed_ids.contains(&o.id))
                .collect();
            if obs.is_empty() {
                break;
            }
            for o in obs {
                let text = format!(
                    "{} {} {}",
                    o.title,
                    o.narrative.as_deref().unwrap_or(""),
                    o.facts.join(" ")
                );
                let emb = Arc::clone(embeddings);
                let embed_result = tokio::task::spawn_blocking(move || emb.embed(&text))
                    .await
                    .unwrap_or_else(
                        |e| Err(anyhow::anyhow!("spawn_blocking failed: {}", e).into()),
                    );

                match embed_result {
                    Ok(vec) => {
                        if self.storage.store_embedding(&o.id, &vec).await.is_ok() {
                            total += 1;
                        } else {
                            failed_ids.insert(o.id.clone());
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to generate embedding for {}: {}", o.id, e);
                        failed_ids.insert(o.id.clone());
                    }
                }
            }
        }
        Ok(total)
    }

    // ── Semantic dedup ──────────────────────────────────────────────────
    pub(crate) async fn deduplicate_by_embedding(
        &self,
        observations: Vec<Observation>,
    ) -> Result<Vec<Observation>, ServiceError> {
        if observations.len() <= 1 {
            return Ok(observations);
        }

        // Without embeddings, skip dedup — just return filtered results
        if self.embeddings.is_none() {
            return Ok(observations);
        }

        let ids: Vec<String> = observations.iter().map(|o| o.id.clone()).collect();
        let embedding_pairs = self.storage.get_embeddings_for_ids(&ids).await?;

        if embedding_pairs.is_empty() {
            return Ok(observations);
        }

        let embedding_map: HashMap<&str, &[f32]> = embedding_pairs
            .iter()
            .map(|(id, vec)| (id.as_str(), vec.as_slice()))
            .collect();

        // Union-find for grouping similar observations
        let obs_count = observations.len();
        let mut parent: Vec<usize> = (0..obs_count).collect();

        // Find root with path compression
        fn find(parent: &mut [usize], mut i: usize) -> usize {
            while let Some(&p) = parent.get(i) {
                if p == i {
                    break;
                }
                // Path compression: read grandparent, then write
                let gp = parent.get(p).copied().unwrap_or(p);
                if let Some(slot) = parent.get_mut(i) {
                    *slot = gp;
                }
                i = p;
            }
            i
        }

        // Compare all pairs and union those above threshold
        for i in 0..obs_count {
            let Some(emb_a) = embedding_map.get(
                observations
                    .get(i)
                    .map(|o| o.id.as_str())
                    .unwrap_or_default(),
            ) else {
                continue;
            };
            for j in (i.checked_add(1).unwrap_or(obs_count))..obs_count {
                let Some(emb_b) = embedding_map.get(
                    observations
                        .get(j)
                        .map(|o| o.id.as_str())
                        .unwrap_or_default(),
                ) else {
                    continue;
                };
                let sim = cosine_similarity(emb_a, emb_b);
                if sim > DEDUP_SIMILARITY_THRESHOLD {
                    let ra = find(&mut parent, i);
                    let rb = find(&mut parent, j);
                    if ra != rb
                        && let Some(slot) = parent.get_mut(rb)
                    {
                        *slot = ra;
                    }
                }
            }
        }

        // Group observations by their root
        let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
        for idx in 0..obs_count {
            let root = find(&mut parent, idx);
            groups.entry(root).or_default().push(idx);
        }

        // For each group, keep the observation with highest noise priority
        // (smallest ordinal: Critical < High < Medium), then most recent as tiebreaker
        let mut kept: Vec<&Observation> = Vec::with_capacity(groups.len());
        for members in groups.values() {
            let best = members
                .iter()
                .filter_map(|&idx| observations.get(idx))
                .min_by(|a, b| {
                    a.noise_level
                        .cmp(&b.noise_level)
                        .then_with(|| b.created_at.cmp(&a.created_at))
                });
            if let Some(obs) = best {
                kept.push(obs);
            }
        }

        // Restore chronological order (most recent first)
        kept.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let deduped_count = obs_count.saturating_sub(kept.len());
        if deduped_count > 0 {
            tracing::debug!(
                original = obs_count,
                deduped = deduped_count,
                remaining = kept.len(),
                "Deduplicated context observations by embedding similarity"
            );
        }

        Ok(kept.into_iter().cloned().collect())
    }
}
