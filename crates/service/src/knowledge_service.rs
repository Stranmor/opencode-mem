use std::sync::Arc;

use opencode_mem_core::{GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType};
use opencode_mem_storage::traits::KnowledgeStore;
use opencode_mem_storage::StorageBackend;

/// Service layer for knowledge operations.
///
/// Wraps all knowledge-related storage calls, providing a single entry point
/// for both HTTP and MCP handlers.
pub struct KnowledgeService {
    storage: Arc<StorageBackend>,
}

impl KnowledgeService {
    #[must_use]
    pub fn new(storage: Arc<StorageBackend>) -> Self {
        Self { storage }
    }

    /// Get a knowledge entry by ID.
    pub async fn get_knowledge(&self, id: &str) -> anyhow::Result<Option<GlobalKnowledge>> {
        self.storage.get_knowledge(id).await
    }

    /// Save or update a knowledge entry (upserts by title).
    pub async fn save_knowledge(&self, input: KnowledgeInput) -> anyhow::Result<GlobalKnowledge> {
        self.storage.save_knowledge(input).await
    }

    /// Delete a knowledge entry by ID. Returns `true` if deleted.
    pub async fn delete_knowledge(&self, id: &str) -> anyhow::Result<bool> {
        self.storage.delete_knowledge(id).await
    }

    /// Full-text search over knowledge entries.
    pub async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<KnowledgeSearchResult>> {
        self.storage.search_knowledge(query, limit).await
    }

    /// List knowledge entries, optionally filtered by type.
    pub async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> anyhow::Result<Vec<GlobalKnowledge>> {
        self.storage.list_knowledge(knowledge_type, limit).await
    }

    /// Increment usage count and bump confidence for a knowledge entry.
    pub async fn update_knowledge_usage(&self, id: &str) -> anyhow::Result<()> {
        self.storage.update_knowledge_usage(id).await
    }
}
