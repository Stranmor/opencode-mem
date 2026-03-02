use async_trait::async_trait;
use opencode_mem_core::UserPrompt;

use crate::error::StorageError;
use crate::pending_queue::PaginatedResult;

/// User prompt operations.
#[async_trait]
pub trait PromptStore: Send + Sync {
    /// Save user prompt.
    async fn save_user_prompt(&self, prompt: &UserPrompt) -> Result<(), StorageError>;

    /// Get prompts with pagination.
    async fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>, StorageError>;

    /// Get prompt by ID.
    async fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>, StorageError>;

    /// Search prompts by text.
    async fn search_prompts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<UserPrompt>, StorageError>;
}
