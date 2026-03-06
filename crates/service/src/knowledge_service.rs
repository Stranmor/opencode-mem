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

    fn fast_fail_if_db_unavailable(&self) -> Result<(), ServiceError> {
        let cb = self.storage.circuit_breaker();
        if cb.should_allow() {
            Ok(())
        } else {
            Err(ServiceError::Storage(StorageError::Unavailable {
                seconds_until_probe: cb.seconds_until_probe(),
            }))
        }
    }

    pub async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        Ok(self.storage.get_knowledge(id).await?)
    }

    pub async fn save_knowledge(
        &self,
        input: KnowledgeInput,
    ) -> Result<GlobalKnowledge, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        Ok(self.storage.save_knowledge(input).await?)
    }

    pub async fn delete_knowledge(&self, id: &str) -> Result<bool, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        Ok(self.storage.delete_knowledge(id).await?)
    }

    pub async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>, ServiceError> {
        let limit = cap_query_limit(limit);
        self.fast_fail_if_db_unavailable()?;
        Ok(self.storage.search_knowledge(query, limit).await?)
    }

    pub async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>, ServiceError> {
        let limit = cap_query_limit(limit);
        self.fast_fail_if_db_unavailable()?;
        Ok(self.storage.list_knowledge(knowledge_type, limit).await?)
    }

    pub async fn update_knowledge_usage(&self, id: &str) -> Result<(), ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        Ok(self.storage.update_knowledge_usage(id).await?)
    }

    pub async fn decay_confidence(&self) -> Result<u64, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        Ok(self.storage.decay_confidence().await?)
    }

    pub async fn auto_archive(&self, min_age_days: i64) -> Result<u64, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        Ok(self.storage.auto_archive(min_age_days).await?)
    }

    pub async fn run_confidence_lifecycle(&self) -> Result<(u64, u64), ServiceError> {
        let decayed = self.decay_confidence().await?;
        let archived = self.auto_archive(30).await?;
        Ok((decayed, archived))
    }
}
