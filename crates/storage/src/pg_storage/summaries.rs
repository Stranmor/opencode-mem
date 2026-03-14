//! SummaryStore implementation for PgStorage.

use super::*;

use crate::error::StorageError;
use crate::pending_queue::PaginatedResult;
use crate::traits::SummaryStore;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use opencode_mem_core::{ProjectId, SessionStatus, SessionSummary, UnsummarizedSession};
use sqlx::Row;

#[async_trait]
impl SummaryStore for PgStorage {
    async fn save_summary(&self, summary: &SessionSummary) -> Result<(), StorageError> {
        sqlx::query(&format!(
            "INSERT INTO session_summaries ({SESSION_SUMMARY_COLUMNS})
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
             ON CONFLICT (session_id) DO UPDATE SET
               project = EXCLUDED.project, request = EXCLUDED.request,
               investigated = EXCLUDED.investigated, learned = EXCLUDED.learned,
               completed = EXCLUDED.completed, next_steps = EXCLUDED.next_steps,
               notes = EXCLUDED.notes, files_read = EXCLUDED.files_read,
               files_edited = EXCLUDED.files_edited, prompt_number = EXCLUDED.prompt_number,
               discovery_tokens = EXCLUDED.discovery_tokens, created_at = EXCLUDED.created_at"
        ))
        .bind(&summary.session_id)
        .bind(&summary.project)
        .bind(&summary.request)
        .bind(&summary.investigated)
        .bind(&summary.learned)
        .bind(&summary.completed)
        .bind(&summary.next_steps)
        .bind(&summary.notes)
        .bind(serde_json::to_value(&summary.files_read)?)
        .bind(serde_json::to_value(&summary.files_edited)?)
        .bind(
            summary
                .prompt_number
                .map(|v| v.as_pg_i32())
                .transpose()
                .map_err(|e| crate::StorageError::DataCorruption {
                    context: e.into(),
                    source: Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()),
                })?,
        )
        .bind(
            summary
                .discovery_tokens
                .map(|v| v.as_pg_i32())
                .transpose()
                .map_err(|e| crate::StorageError::DataCorruption {
                    context: e.into(),
                    source: Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()),
                })?,
        )
        .bind(summary.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_summary(&self, session_id: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM session_summaries WHERE session_id = $1")
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_session_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionSummary>, StorageError> {
        let row = sqlx::query(&format!(
            "SELECT {SESSION_SUMMARY_COLUMNS} FROM session_summaries WHERE session_id = $1"
        ))
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| row_to_summary(&r)).transpose()
    }

    async fn update_session_status_with_summary(
        &self,
        session_id: &str,
        status: SessionStatus,
        summary: Option<&str>,
    ) -> Result<(), StorageError> {
        let mut tx = self.pool.begin().await?;
        let ended_at: Option<DateTime<Utc>> = (status != SessionStatus::Active).then(Utc::now);

        let rows_affected =
            sqlx::query("UPDATE sessions SET status = $1, ended_at = $2 WHERE id = $3")
                .bind(status.as_str())
                .bind(ended_at)
                .bind(session_id)
                .execute(&mut *tx)
                .await?
                .rows_affected();

        if rows_affected == 0 {
            tracing::warn!(
                session_id,
                "update_session_status_with_summary: session not found"
            );
        }

        if let Some(s) = summary {
            let project: Option<Option<String>> =
                sqlx::query_scalar("SELECT project FROM sessions WHERE id = $1")
                    .bind(session_id)
                    .fetch_optional(&mut *tx)
                    .await?;

            // project is Some(Some(proj)) if row found with non-null project,
            // Some(None) if row found with null project, None if no row.
            if let Some(proj) = project {
                let now = Utc::now();
                let empty_json = serde_json::json!([]);
                sqlx::query(&format!(
                    "INSERT INTO session_summaries ({SESSION_SUMMARY_COLUMNS})
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
                     ON CONFLICT (session_id) DO UPDATE SET
                       learned = EXCLUDED.learned, created_at = EXCLUDED.created_at"
                ))
                .bind(session_id)
                .bind(proj)
                .bind(Option::<String>::None)
                .bind(Option::<String>::None)
                .bind(Some(s))
                .bind(Option::<String>::None)
                .bind(Option::<String>::None)
                .bind(Option::<String>::None)
                .bind(&empty_json)
                .bind(&empty_json)
                .bind(Option::<i32>::None)
                .bind(Option::<i32>::None)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;
        Ok(())
    }

    async fn get_summaries_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<SessionSummary>, StorageError> {
        let total: i64 = if let Some(p) = project {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM session_summaries WHERE (project = $1 OR project IS NULL)",
            )
            .bind(p)
            .fetch_one(&self.pool)
            .await?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM session_summaries")
                .fetch_one(&self.pool)
                .await?
        };

        let rows = if let Some(p) = project {
            sqlx::query(&format!(
                "SELECT {SESSION_SUMMARY_COLUMNS} FROM session_summaries
                 WHERE (project = $1 OR project IS NULL) ORDER BY created_at DESC, session_id ASC LIMIT $2 OFFSET $3"
            ))
            .bind(p)
            .bind(usize_to_i64(limit))
            .bind(usize_to_i64(offset))
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(&format!(
                "SELECT {SESSION_SUMMARY_COLUMNS} FROM session_summaries
                 ORDER BY created_at DESC, session_id ASC LIMIT $1 OFFSET $2"
            ))
            .bind(usize_to_i64(limit))
            .bind(usize_to_i64(offset))
            .fetch_all(&self.pool)
            .await?
        };

        let items: Vec<SessionSummary> = collect_skipping_corrupt(rows.iter().map(row_to_summary))?;
        Ok(PaginatedResult::new(items, total, offset, limit))
    }

    async fn search_sessions(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SessionSummary>, StorageError> {
        let Some(tsquery) = build_tsquery(query) else {
            return Ok(Vec::new());
        };
        let rows = sqlx::query(&format!(
            "SELECT {SESSION_SUMMARY_COLUMNS} FROM session_summaries
             WHERE search_vec @@ to_tsquery('simple', $1)
             ORDER BY ts_rank_cd(search_vec, to_tsquery('simple', $1)) DESC
             LIMIT $2"
        ))
        .bind(&tsquery)
        .bind(usize_to_i64(limit))
        .fetch_all(&self.pool)
        .await?;
        Ok(collect_skipping_corrupt(rows.iter().map(row_to_summary))?)
    }

    async fn get_sessions_without_summaries(
        &self,
        limit: usize,
    ) -> Result<Vec<UnsummarizedSession>, StorageError> {
        let rows = sqlx::query(
            "SELECT o.session_id,
                    MIN(o.project) as project,
                    COUNT(*) as obs_count,
                    MAX(o.created_at) as last_obs
             FROM observations o
             WHERE o.session_id != 'manual'
               AND NOT EXISTS (
                   SELECT 1 FROM session_summaries ss
                   WHERE ss.session_id = o.session_id
                     AND NOT (ss.learned = 'processing'
                              AND ss.created_at < NOW() - INTERVAL '10 minutes')
               )
             GROUP BY o.session_id
             HAVING MAX(o.created_at) < NOW() - INTERVAL '1 hour' AND COUNT(*) >= 2
             ORDER BY last_obs ASC
             LIMIT $1",
        )
        .bind(usize_to_i64(limit))
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::with_capacity(rows.len());
        for row in &rows {
            let session_id: String = row.try_get("session_id")?;
            let project: Option<String> = row.try_get("project")?;
            let obs_count: i64 = row.try_get("obs_count")?;
            let last_obs: DateTime<Utc> = row.try_get("last_obs")?;
            results.push(UnsummarizedSession::new(
                session_id,
                project.map(ProjectId::new),
                usize::try_from(obs_count).unwrap_or(0),
                last_obs,
            ));
        }
        Ok(results)
    }
}
