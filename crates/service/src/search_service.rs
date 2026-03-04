use std::collections::HashMap;
use std::sync::Arc;

use opencode_mem_core::{
    cosine_similarity, GlobalKnowledge, KnowledgeSearchResult, KnowledgeType, Observation,
    SearchResult, SessionSummary, UserPrompt,
};
use opencode_mem_embeddings::{EmbeddingProvider, EmbeddingService};
use opencode_mem_infinite::InfiniteMemory;
use opencode_mem_storage::traits::{
    EmbeddingStore, KnowledgeStore, ObservationStore, PromptStore, SearchStore, StatsStore,
    SummaryStore,
};
use opencode_mem_storage::{CircuitBreaker, PaginatedResult, StorageBackend, StorageStats};

use crate::ServiceError;

/// Cosine similarity threshold above which two observations are considered semantic duplicates.
const DEDUP_SIMILARITY_THRESHOLD: f32 = 0.85;

pub struct SearchService {
    storage: Arc<StorageBackend>,
    embeddings: Option<Arc<EmbeddingService>>,
    hybrid_search: opencode_mem_search::HybridSearch,
    infinite_mem: Option<Arc<InfiniteMemory>>,
}

impl SearchService {
    #[must_use]
    pub fn new(
        storage: Arc<StorageBackend>,
        embeddings: Option<Arc<EmbeddingService>>,
        infinite_mem: Option<Arc<InfiniteMemory>>,
    ) -> Self {
        let hybrid_search =
            opencode_mem_search::HybridSearch::new(storage.clone(), embeddings.clone());
        Self { storage, embeddings, hybrid_search, infinite_mem }
    }

    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        self.storage.circuit_breaker()
    }

    async fn with_cb<T>(&self, result: Result<T, ServiceError>) -> Result<T, ServiceError> {
        match &result {
            Ok(_) => {
                let recovered = self.storage.circuit_breaker().record_success();
                if recovered {
                    self.on_recovery();
                }
            },
            Err(e) if e.is_db_unavailable() => self.storage.circuit_breaker().record_failure(),
            Err(_) => {},
        }
        result
    }

    fn on_recovery(&self) {
        if self.storage.has_pending_migrations() {
            let storage = self.storage.clone();
            tokio::spawn(async move {
                let _ = storage.try_run_migrations().await;
            });
        }

        if let Some(ref im) = self.infinite_mem {
            if im.has_pending_migrations() {
                let im = Arc::clone(im);
                tokio::spawn(async move {
                    im.try_run_migrations().await;
                });
            }
        }
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
        let result = self
            .hybrid_search
            .search_with_filters(query, project, obs_type, from, to, limit)
            .await
            .map_err(ServiceError::Search);
        self.with_cb(result).await
    }

    pub async fn hybrid_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        let result = self.hybrid_search.search(query, limit).await.map_err(ServiceError::Search);
        self.with_cb(result).await
    }

    pub async fn get_timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        let result = self.storage.get_timeline(from, to, limit).await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn semantic_search_with_fallback(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        let result = self
            .hybrid_search
            .semantic_search_with_fallback(query, limit)
            .await
            .map_err(ServiceError::Search);
        self.with_cb(result).await
    }

    // ── ObservationStore read delegates ─────────────────────────────────

    pub async fn get_observation_by_id(
        &self,
        id: &str,
    ) -> Result<Option<Observation>, ServiceError> {
        let result = self.storage.get_by_id(id).await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_recent_observations(
        &self,
        limit: usize,
    ) -> Result<Vec<Observation>, ServiceError> {
        let result = self.storage.get_recent(limit).await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_observations_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<Observation>, ServiceError> {
        let result = self.storage.get_observations_by_ids(ids).await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_context_for_project(
        &self,
        project: &str,
        limit: usize,
    ) -> Result<Vec<Observation>, ServiceError> {
        let result =
            self.storage.get_context_for_project(project, limit).await.map_err(ServiceError::from);
        let observations = self.with_cb(result).await?;
        self.deduplicate_by_embedding(observations).await
    }

    pub async fn search_by_file(
        &self,
        file_path: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        let result =
            self.storage.search_by_file(file_path, limit).await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    // ── StatsStore delegates ───────────────────────────────────────────

    pub async fn get_stats(&self) -> Result<StorageStats, ServiceError> {
        let result = self.storage.get_stats().await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_all_projects(&self) -> Result<Vec<String>, ServiceError> {
        let result = self.storage.get_all_projects().await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_observations_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<Observation>, ServiceError> {
        let result = self
            .storage
            .get_observations_paginated(offset, limit, project)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    // ── SummaryStore delegates ─────────────────────────────────────────

    pub async fn search_sessions(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SessionSummary>, ServiceError> {
        let result = self.storage.search_sessions(query, limit).await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_session_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionSummary>, ServiceError> {
        let result = self.storage.get_session_summary(session_id).await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_summaries_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<SessionSummary>, ServiceError> {
        let result = self
            .storage
            .get_summaries_paginated(offset, limit, project)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    // ── PromptStore delegates ──────────────────────────────────────────

    pub async fn search_prompts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<UserPrompt>, ServiceError> {
        let result = self.storage.search_prompts(query, limit).await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>, ServiceError> {
        let result = self.storage.get_prompt_by_id(id).await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>, ServiceError> {
        let result = self
            .storage
            .get_prompts_paginated(offset, limit, project)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    // ── KnowledgeStore delegates (read-only) ───────────────────────────

    pub async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>, ServiceError> {
        let result = self.storage.search_knowledge(query, limit).await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>, ServiceError> {
        let result =
            self.storage.list_knowledge(knowledge_type, limit).await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>, ServiceError> {
        let result = self.storage.get_knowledge(id).await.map_err(ServiceError::from);
        self.with_cb(result).await
    }

    // ── EmbeddingStore delegates ────────────────────────────────────────

    pub async fn clear_embeddings(&self) -> Result<(), ServiceError> {
        let result = self.storage.clear_embeddings().await.map_err(ServiceError::from);
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
            let all_obs = self.storage.get_observations_without_embeddings(batch_size).await?;
            if all_obs.is_empty() {
                break;
            }
            let obs: Vec<_> = all_obs.into_iter().filter(|o| !failed_ids.contains(&o.id)).collect();
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
                let embed_result =
                    tokio::task::spawn_blocking(move || emb.embed(&text)).await.unwrap_or_else(
                        |e| Err(anyhow::anyhow!("spawn_blocking failed: {}", e).into()),
                    );

                match embed_result {
                    Ok(vec) => {
                        if self.storage.store_embedding(&o.id, &vec).await.is_ok() {
                            total += 1;
                        } else {
                            failed_ids.insert(o.id.clone());
                        }
                    },
                    Err(e) => {
                        tracing::warn!("Failed to generate embedding for {}: {}", o.id, e);
                        failed_ids.insert(o.id.clone());
                    },
                }
            }
        }
        Ok(total)
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
