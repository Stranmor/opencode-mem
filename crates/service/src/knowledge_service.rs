use std::sync::Arc;

use opencode_mem_core::{GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType};
use opencode_mem_storage::traits::KnowledgeStore;
use opencode_mem_storage::StorageBackend;

use crate::ServiceError;

pub struct KnowledgeService {
    storage: Arc<StorageBackend>,
}

impl KnowledgeService {
    #[must_use]
    pub fn new(storage: Arc<StorageBackend>) -> Self {
        Self { storage }
    }

    pub async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>, ServiceError> {
        Ok(self.storage.get_knowledge(id).await?)
    }

    pub async fn save_knowledge(
        &self,
        input: KnowledgeInput,
    ) -> Result<GlobalKnowledge, ServiceError> {
        Ok(self.storage.save_knowledge(input).await?)
    }

    pub async fn delete_knowledge(&self, id: &str) -> Result<bool, ServiceError> {
        Ok(self.storage.delete_knowledge(id).await?)
    }

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

    pub async fn update_knowledge_usage(&self, id: &str) -> Result<(), ServiceError> {
        Ok(self.storage.update_knowledge_usage(id).await?)
    }
}
