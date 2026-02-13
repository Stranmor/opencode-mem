//! PromptStore implementation for PgStorage.

use super::*;

use crate::pending_queue::PaginatedResult;
use crate::traits::PromptStore;
use async_trait::async_trait;

#[async_trait]
impl PromptStore for PgStorage {
    async fn save_user_prompt(&self, prompt: &UserPrompt) -> Result<()> {
        sqlx::query(
            "INSERT INTO user_prompts (id, content_session_id, prompt_number, prompt_text, project, created_at)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT (id) DO UPDATE SET
                 prompt_text = EXCLUDED.prompt_text, project = EXCLUDED.project",
        )
        .bind(&prompt.id)
        .bind(&prompt.content_session_id)
        .bind(prompt.prompt_number as i32)
        .bind(&prompt.prompt_text)
        .bind(&prompt.project)
        .bind(prompt.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>> {
        let total: i64 = if let Some(p) = project {
            sqlx::query_scalar("SELECT COUNT(*) FROM user_prompts WHERE project = $1")
                .bind(p)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM user_prompts").fetch_one(&self.pool).await?
        };

        let rows = if let Some(p) = project {
            sqlx::query(
                "SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
                   FROM user_prompts WHERE project = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
            )
            .bind(p)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
                   FROM user_prompts ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            )
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?
        };

        let items: Vec<UserPrompt> = rows.iter().map(row_to_prompt).collect::<Result<_>>()?;
        Ok(PaginatedResult {
            items,
            total: total as u64,
            offset: offset as u64,
            limit: limit as u64,
        })
    }

    async fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>> {
        let row = sqlx::query(
            "SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
               FROM user_prompts WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_prompt).transpose()
    }

    async fn search_prompts(&self, query: &str, limit: usize) -> Result<Vec<UserPrompt>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let pattern = format!("%{}%", escape_like(query));
        let rows = sqlx::query(
            "SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
               FROM user_prompts
               WHERE prompt_text ILIKE $1
               ORDER BY created_at DESC
               LIMIT $2",
        )
        .bind(&pattern)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_prompt).collect()
    }
}
