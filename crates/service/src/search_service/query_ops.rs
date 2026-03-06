//! Knowledge, session, and prompt query methods for SearchService.

use opencode_mem_core::{
    GlobalKnowledge, KnowledgeSearchResult, KnowledgeType, SessionSummary, UserPrompt,
};
use opencode_mem_storage::{
    PaginatedResult,
    traits::{KnowledgeStore, PromptStore, SummaryStore},
};

use crate::ServiceError;

use super::SearchService;

impl SearchService {
    pub async fn search_sessions(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SessionSummary>, ServiceError> {
        let limit = Self::normalize_limit(limit);
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .search_sessions(query, limit)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_session_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionSummary>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .get_session_summary(session_id)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_summaries_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<SessionSummary>, ServiceError> {
        let limit = Self::normalize_limit(limit);
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .get_summaries_paginated(offset, limit, project)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn search_prompts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<UserPrompt>, ServiceError> {
        let limit = Self::normalize_limit(limit);
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .search_prompts(query, limit)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .get_prompt_by_id(id)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>, ServiceError> {
        let limit = Self::normalize_limit(limit);
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .get_prompts_paginated(offset, limit, project)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>, ServiceError> {
        let limit = Self::normalize_limit(limit);
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .search_knowledge(query, limit)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>, ServiceError> {
        let limit = Self::normalize_limit(limit);
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .list_knowledge(knowledge_type, limit)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }

    pub async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .get_knowledge(id)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result).await
    }
}
