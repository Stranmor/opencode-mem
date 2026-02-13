//! StatsStore implementation for PgStorage.

use super::*;

use crate::pending_queue::{PaginatedResult, StorageStats};
use crate::traits::StatsStore;
use async_trait::async_trait;

#[async_trait]
impl StatsStore for PgStorage {
    async fn get_stats(&self) -> Result<StorageStats> {
        let observation_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM observations").fetch_one(&self.pool).await?;
        let session_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sessions").fetch_one(&self.pool).await?;
        let summary_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_summaries")
            .fetch_one(&self.pool)
            .await?;
        let prompt_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM user_prompts").fetch_one(&self.pool).await?;
        let project_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT project) FROM observations WHERE project IS NOT NULL",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(StorageStats {
            observation_count: observation_count as u64,
            session_count: session_count as u64,
            summary_count: summary_count as u64,
            prompt_count: prompt_count as u64,
            project_count: project_count as u64,
        })
    }

    async fn get_all_projects(&self) -> Result<Vec<String>> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT project FROM observations WHERE project IS NOT NULL ORDER BY project",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn get_observations_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<Observation>> {
        let total: i64 = if let Some(p) = project {
            sqlx::query_scalar("SELECT COUNT(*) FROM observations WHERE project = $1")
                .bind(p)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM observations").fetch_one(&self.pool).await?
        };

        let rows = if let Some(p) = project {
            sqlx::query(
                "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                        facts, concepts, files_read, files_modified, keywords, prompt_number,
                        discovery_tokens, noise_level, noise_reason, created_at
                   FROM observations WHERE project = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
            )
            .bind(p)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                        facts, concepts, files_read, files_modified, keywords, prompt_number,
                        discovery_tokens, noise_level, noise_reason, created_at
                   FROM observations ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            )
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?
        };
        let items: Vec<Observation> = rows.iter().map(row_to_observation).collect::<Result<_>>()?;
        Ok(PaginatedResult {
            items,
            total: total as u64,
            offset: offset as u64,
            limit: limit as u64,
        })
    }
}
