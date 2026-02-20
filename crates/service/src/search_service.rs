use std::collections::HashMap;
use std::sync::Arc;

use opencode_mem_core::{
    cosine_similarity, GlobalKnowledge, KnowledgeSearchResult, KnowledgeType, Observation,
    SearchResult, SessionSummary, UserPrompt,
};
use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_storage::traits::{
    EmbeddingStore, KnowledgeStore, ObservationStore, PromptStore, SearchStore, StatsStore,
    SummaryStore,
};
use opencode_mem_storage::{PaginatedResult, StorageBackend, StorageStats};

use crate::ServiceError;

/// Cosine similarity threshold above which two observations are considered semantic duplicates.
const DEDUP_SIMILARITY_THRESHOLD: f32 = 0.85;

pub struct SearchService {
    storage: Arc<StorageBackend>,
    embeddings: Option<Arc<EmbeddingService>>,
}

impl SearchService {
    #[must_use]
    pub fn new(storage: Arc<StorageBackend>, embeddings: Option<Arc<EmbeddingService>>) -> Self {
        Self { storage, embeddings }
    }

    // ── SearchStore delegates ──────────────────────────────────────────

    pub async fn search_with_filters(
        &self,
        query: Option<&str>,
        project: Option<&str>,
        obs_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        Ok(self.storage.search_with_filters(query, project, obs_type, from, to, limit).await?)
    }

    pub async fn hybrid_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        Ok(self.storage.hybrid_search(query, limit).await?)
    }

    pub async fn get_timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        Ok(self.storage.get_timeline(from, to, limit).await?)
    }

    pub async fn semantic_search_with_fallback(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        opencode_mem_search::run_semantic_search_with_fallback(
            &self.storage,
            self.embeddings.as_deref(),
            query,
            limit,
        )
        .await
        .map_err(ServiceError::Search)
    }

    // ── ObservationStore read delegates ─────────────────────────────────

    pub async fn get_observation_by_id(
        &self,
        id: &str,
    ) -> Result<Option<Observation>, ServiceError> {
        Ok(self.storage.get_by_id(id).await?)
    }

    pub async fn get_recent_observations(
        &self,
        limit: usize,
    ) -> Result<Vec<Observation>, ServiceError> {
        Ok(self.storage.get_recent(limit).await?)
    }

    pub async fn get_observations_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<Observation>, ServiceError> {
        Ok(self.storage.get_observations_by_ids(ids).await?)
    }

    pub async fn get_context_for_project(
        &self,
        project: &str,
        limit: usize,
    ) -> Result<Vec<Observation>, ServiceError> {
        let observations = self.storage.get_context_for_project(project, limit).await?;
        self.deduplicate_by_embedding(observations).await
    }

    pub async fn search_by_file(
        &self,
        file_path: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        Ok(self.storage.search_by_file(file_path, limit).await?)
    }

    // ── StatsStore delegates ───────────────────────────────────────────

    pub async fn get_stats(&self) -> Result<StorageStats, ServiceError> {
        Ok(self.storage.get_stats().await?)
    }

    pub async fn get_all_projects(&self) -> Result<Vec<String>, ServiceError> {
        Ok(self.storage.get_all_projects().await?)
    }

    pub async fn get_observations_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<Observation>, ServiceError> {
        Ok(self.storage.get_observations_paginated(offset, limit, project).await?)
    }

    // ── SummaryStore delegates ─────────────────────────────────────────

    pub async fn search_sessions(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SessionSummary>, ServiceError> {
        Ok(self.storage.search_sessions(query, limit).await?)
    }

    pub async fn get_session_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionSummary>, ServiceError> {
        Ok(self.storage.get_session_summary(session_id).await?)
    }

    pub async fn get_summaries_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<SessionSummary>, ServiceError> {
        Ok(self.storage.get_summaries_paginated(offset, limit, project).await?)
    }

    // ── PromptStore delegates ──────────────────────────────────────────

    pub async fn search_prompts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<UserPrompt>, ServiceError> {
        Ok(self.storage.search_prompts(query, limit).await?)
    }

    pub async fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>, ServiceError> {
        Ok(self.storage.get_prompt_by_id(id).await?)
    }

    pub async fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>, ServiceError> {
        Ok(self.storage.get_prompts_paginated(offset, limit, project).await?)
    }

    // ── KnowledgeStore delegates (read-only) ───────────────────────────

    pub async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>, ServiceError> {
        Ok(self.storage.search_knowledge(query, limit).await?)
    }

    pub async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>, ServiceError> {
        Ok(self.storage.list_knowledge(knowledge_type, limit).await?)
    }

    pub async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>, ServiceError> {
        Ok(self.storage.get_knowledge(id).await?)
    }

    // ── EmbeddingStore delegates ────────────────────────────────────────

    pub async fn clear_embeddings(&self) -> Result<(), ServiceError> {
        Ok(self.storage.clear_embeddings().await?)
    }

    // ── Semantic dedup ──────────────────────────────────────────────────

    async fn deduplicate_by_embedding(
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

        let embedding_map: HashMap<&str, &[f32]> =
            embedding_pairs.iter().map(|(id, vec)| (id.as_str(), vec.as_slice())).collect();

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
            let Some(emb_a) =
                embedding_map.get(observations.get(i).map(|o| o.id.as_str()).unwrap_or_default())
            else {
                continue;
            };
            for j in (i.checked_add(1).unwrap_or(obs_count))..obs_count {
                let Some(emb_b) = embedding_map
                    .get(observations.get(j).map(|o| o.id.as_str()).unwrap_or_default())
                else {
                    continue;
                };
                let sim = cosine_similarity(emb_a, emb_b);
                if sim > DEDUP_SIMILARITY_THRESHOLD {
                    let ra = find(&mut parent, i);
                    let rb = find(&mut parent, j);
                    if ra != rb {
                        if let Some(slot) = parent.get_mut(rb) {
                            *slot = ra;
                        }
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
            let best = members.iter().filter_map(|&idx| observations.get(idx)).min_by(|a, b| {
                a.noise_level.cmp(&b.noise_level).then_with(|| b.created_at.cmp(&a.created_at))
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
