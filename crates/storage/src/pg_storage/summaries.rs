//! SummaryStore implementation for PgStorage.

use super::*;

use crate::pending_queue::PaginatedResult;
use crate::traits::SummaryStore;
use async_trait::async_trait;

#[async_trait]
impl SummaryStore for PgStorage {
    async fn save_summary(&self, summary: &SessionSummary) -> Result<()> {
        sqlx::query(&format!(
            "INSERT INTO session_summaries ({SUMMARY_COLUMNS})
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
        .bind(summary.prompt_number.map(|v| i32::try_from(v).unwrap_or(i32::MAX)))
        .bind(summary.discovery_tokens.map(|v| i32::try_from(v).unwrap_or(i32::MAX)))
        .bind(summary.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_session_summary(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        let row = sqlx::query(&format!(
            "SELECT {SUMMARY_COLUMNS} FROM session_summaries WHERE session_id = $1"
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
    ) -> Result<()> {
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
            tracing::warn!(session_id, "update_session_status_with_summary: session not found");
        }

        if let Some(s) = summary {
            let project: Option<Option<String>> =
                sqlx::query_scalar("SELECT project FROM sessions WHERE id = $1")
                    .bind(session_id)
                    .fetch_optional(&mut *tx)
                    .await?;

            // project is Some(Some(proj)) if row found with non-null project,
            // Some(None) if row found with null project, None if no row.
            // Flatten: use empty string for null project since session_summaries.project is nullable.
            if let Some(maybe_proj) = project {
                let proj = maybe_proj.unwrap_or_default();
                let now = Utc::now();
                let empty_json = serde_json::json!([]);
                sqlx::query(&format!(
                    "INSERT INTO session_summaries ({SUMMARY_COLUMNS})
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
    ) -> Result<PaginatedResult<SessionSummary>> {
        let total: i64 = if let Some(p) = project {
            sqlx::query_scalar("SELECT COUNT(*) FROM session_summaries WHERE project = $1")
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
                "SELECT {SUMMARY_COLUMNS} FROM session_summaries
                 WHERE project = $1 ORDER BY created_at DESC, session_id ASC LIMIT $2 OFFSET $3"
            ))
            .bind(p)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(&format!(
                "SELECT {SUMMARY_COLUMNS} FROM session_summaries
                 ORDER BY created_at DESC, session_id ASC LIMIT $1 OFFSET $2"
            ))
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?
        };

        let items: Vec<SessionSummary> = rows.iter().map(row_to_summary).collect::<Result<_>>()?;
        Ok(PaginatedResult {
            items,
            total: total as u64,
            offset: offset as u64,
            limit: limit as u64,
        })
    }

    async fn search_sessions(&self, query: &str, limit: usize) -> Result<Vec<SessionSummary>> {
        let tsquery = build_tsquery(query);
        if tsquery.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(&format!(
            "SELECT {SUMMARY_COLUMNS} FROM session_summaries
             WHERE search_vec @@ to_tsquery('english', $1)
             ORDER BY ts_rank_cd(search_vec, to_tsquery('english', $1)) DESC
             LIMIT $2"
        ))
        .bind(&tsquery)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_summary).collect()
    }
}
