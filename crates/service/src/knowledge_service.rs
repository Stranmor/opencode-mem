use std::sync::Arc;

use opencode_mem_core::{
    GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType, cap_query_limit,
};
use opencode_mem_embeddings::LazyEmbeddingService;
use opencode_mem_storage::traits::{EmbeddingStore, KnowledgeStore};
use opencode_mem_storage::{StorageBackend, StorageError};

use crate::ServiceError;

const PROVENANCE_SIMILARITY_THRESHOLD: f32 = 0.75;

#[derive(Clone)]
pub struct KnowledgeService {
    storage: Arc<StorageBackend>,
    embeddings: Option<Arc<LazyEmbeddingService>>,
}

impl KnowledgeService {
    #[must_use]
    pub fn new(
        storage: Arc<StorageBackend>,
        embeddings: Option<Arc<LazyEmbeddingService>>,
    ) -> Self {
        Self {
            storage,
            embeddings,
        }
    }

    pub fn circuit_breaker(&self) -> &opencode_mem_storage::CircuitBreaker {
        self.storage.circuit_breaker()
    }

    pub(crate) fn with_cb<T>(&self, result: Result<T, StorageError>) -> Result<T, ServiceError> {
        result.map_err(ServiceError::from)
    }

    pub async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.get_knowledge(id))
            .await;
        let knowledge = self.with_cb(result)?;
        if knowledge.is_some() {
            self.spawn_usage_increment(vec![id.to_owned()]);
        }
        Ok(knowledge)
    }

    pub async fn save_knowledge(
        &self,
        input: KnowledgeInput,
    ) -> Result<GlobalKnowledge, ServiceError> {
        self.save_knowledge_with_id(&uuid::Uuid::new_v4().to_string(), input)
            .await
    }

    pub async fn save_knowledge_with_id(
        &self,
        id: &str,
        input: KnowledgeInput,
    ) -> Result<GlobalKnowledge, ServiceError> {
        let needs_provenance = input.source_observation.is_none() && self.embeddings.is_some();

        let embedding = self.generate_knowledge_embedding(&input).await;

        let result = if let Some(emb) = embedding {
            self.storage
                .guarded(|| {
                    self.storage
                        .save_knowledge_with_embedding(id, input.clone(), emb.clone())
                })
                .await
        } else {
            self.storage
                .guarded(|| self.storage.save_knowledge_with_id(id, input.clone()))
                .await
        };
        let knowledge = self.with_cb(result)?;

        if needs_provenance {
            self.spawn_provenance_linking(knowledge.clone());
        }

        Ok(knowledge)
    }

    pub async fn delete_knowledge(&self, id: &str) -> Result<bool, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.delete_knowledge(id))
            .await;
        self.with_cb(result)
    }

    pub async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>, ServiceError> {
        let limit = cap_query_limit(limit);
        let result = self
            .storage
            .guarded(|| self.storage.search_knowledge(query, limit))
            .await;
        let results = self.with_cb(result)?;
        let ids: Vec<String> = results.iter().map(|r| r.knowledge.id.clone()).collect();
        if !ids.is_empty() {
            self.spawn_usage_increment(ids);
        }
        Ok(results)
    }

    pub async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>, ServiceError> {
        let limit = cap_query_limit(limit);
        let result = self
            .storage
            .guarded(|| self.storage.list_knowledge(knowledge_type, limit))
            .await;
        self.with_cb(result)
    }

    pub async fn update_knowledge_usage(&self, id: &str) -> Result<(), ServiceError> {
        self.update_knowledge_usage_batch(&[id.to_owned()]).await
    }

    async fn generate_knowledge_embedding(&self, input: &KnowledgeInput) -> Option<Vec<f32>> {
        let embeddings = self.embeddings.as_ref()?;
        let text = format!("{} {}", input.title.trim(), input.description);
        let embeddings_clone = Arc::clone(embeddings);
        let embed_result = tokio::task::spawn_blocking(move || {
            use opencode_mem_embeddings::EmbeddingProvider;
            embeddings_clone.embed(&text)
        })
        .await;

        match embed_result {
            Ok(Ok(vec)) => Some(vec),
            Ok(Err(e)) => {
                tracing::warn!(
                    title = %input.title,
                    error = %e,
                    "Knowledge embedding generation failed, falling back to non-semantic dedup"
                );
                None
            }
            Err(e) => {
                tracing::warn!(
                    title = %input.title,
                    error = %e,
                    "Knowledge embedding spawn_blocking panicked"
                );
                None
            }
        }
    }

    fn spawn_usage_increment(&self, ids: Vec<String>) {
        let storage = Arc::clone(&self.storage);
        tokio::spawn(async move {
            if let Err(e) = storage.update_knowledge_usage_batch(&ids).await {
                tracing::warn!(
                    error = %e,
                    count = ids.len(),
                    "Failed to increment knowledge usage_count"
                );
            }
        });
    }

    fn spawn_provenance_linking(&self, knowledge: GlobalKnowledge) {
        let Some(ref embeddings) = self.embeddings else {
            return;
        };
        if !knowledge.source_observations.is_empty() {
            return;
        }

        let embeddings = Arc::clone(embeddings);
        let storage = Arc::clone(&self.storage);
        let knowledge_id = knowledge.id.clone();

        tokio::spawn(async move {
            let text = format!("{} {}", knowledge.title, knowledge.description);
            let embeddings_clone = Arc::clone(&embeddings);
            let embed_result = tokio::task::spawn_blocking(move || {
                use opencode_mem_embeddings::EmbeddingProvider;
                embeddings_clone.embed(&text)
            })
            .await;

            let embedding = match embed_result {
                Ok(Ok(vec)) => vec,
                Ok(Err(e)) => {
                    tracing::warn!(
                        knowledge_id = %knowledge_id,
                        error = %e,
                        "Provenance linking: embedding generation failed"
                    );
                    return;
                }
                Err(e) => {
                    tracing::warn!(
                        knowledge_id = %knowledge_id,
                        error = %e,
                        "Provenance linking: spawn_blocking panicked"
                    );
                    return;
                }
            };

            let similar = match storage
                .find_similar(&embedding, PROVENANCE_SIMILARITY_THRESHOLD, None)
                .await
            {
                Ok(Some(m)) => m,
                Ok(None) => return,
                Err(e) => {
                    tracing::warn!(
                        knowledge_id = %knowledge_id,
                        error = %e,
                        "Provenance linking: find_similar failed"
                    );
                    return;
                }
            };

            match storage
                .link_source_observation(&knowledge_id, &similar.observation_id)
                .await
            {
                Ok(true) => {
                    tracing::info!(
                        knowledge_id = %knowledge_id,
                        observation_id = %similar.observation_id,
                        similarity = %similar.similarity,
                        "Auto-linked provenance via embedding similarity"
                    );
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(
                        knowledge_id = %knowledge_id,
                        error = %e,
                        "Provenance linking: link_source_observation failed"
                    );
                }
            }
        });
    }

    pub async fn update_knowledge_usage_batch(&self, ids: &[String]) -> Result<(), ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.update_knowledge_usage_batch(ids))
            .await;
        self.with_cb(result)
    }

    pub async fn decay_confidence(&self) -> Result<u64, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.decay_confidence())
            .await;
        self.with_cb(result)
    }

    pub async fn auto_archive(&self, min_age_days: i64) -> Result<u64, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.auto_archive(min_age_days))
            .await;
        self.with_cb(result)
    }

    pub async fn run_confidence_lifecycle(&self) -> Result<(u64, u64), ServiceError> {
        let decayed = self.decay_confidence().await?;
        let archived = self.auto_archive(90).await?;
        Ok((decayed, archived))
    }
}
