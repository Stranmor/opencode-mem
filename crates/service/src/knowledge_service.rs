use std::sync::Arc;

use opencode_mem_core::{
    GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType, cap_query_limit,
};
use opencode_mem_storage::traits::KnowledgeStore;
use opencode_mem_storage::{StorageBackend, StorageError};

use crate::ServiceError;

#[derive(Clone)]
pub struct KnowledgeService {
    storage: Arc<StorageBackend>,
}

impl KnowledgeService {
    #[must_use]
    pub fn new(storage: Arc<StorageBackend>) -> Self {
        Self { storage }
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
        let result = self
            .storage
            .guarded(|| self.storage.save_knowledge_with_id(id, input.clone()))
            .await;
        self.with_cb(result)
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

    /// Fire-and-forget usage_count increment. Failures are logged at warn level
    /// and never propagate to the caller — reading knowledge must never fail
    /// because of a usage tracking issue.
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
        let archived = self.auto_archive(30).await?;
        Ok((decayed, archived))
    }
}
