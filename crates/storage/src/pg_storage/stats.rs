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
            observation_count: u64::try_from(observation_count).unwrap_or(0),
            session_count: u64::try_from(session_count).unwrap_or(0),
            summary_count: u64::try_from(summary_count).unwrap_or(0),
            prompt_count: u64::try_from(prompt_count).unwrap_or(0),
            project_count: u64::try_from(project_count).unwrap_or(0),
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
            .bind(usize_to_i64(limit))
            .bind(usize_to_i64(offset))
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                        facts, concepts, files_read, files_modified, keywords, prompt_number,
                        discovery_tokens, noise_level, noise_reason, created_at
                   FROM observations ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            )
            .bind(usize_to_i64(limit))
            .bind(usize_to_i64(offset))
            .fetch_all(&self.pool)
            .await?
        };
        let items: Vec<Observation> = rows.iter().map(row_to_observation).collect::<Result<_>>()?;
        Ok(PaginatedResult {
            items,
            total: u64::try_from(total).unwrap_or(0),
            offset: u64::try_from(offset).unwrap_or(0),
            limit: u64::try_from(limit).unwrap_or(0),
        })
    }
}
