//! PendingQueueStore implementation for PgStorage.

use super::*;

use crate::pending_queue::{QueueStats, max_retry_count};
use crate::traits::PendingQueueStore;
use async_trait::async_trait;

#[async_trait]
impl PendingQueueStore for PgStorage {
    async fn queue_message(
        &self,
        session_id: &str,
        tool_name: Option<&str>,
        tool_input: Option<&str>,
        tool_response: Option<&str>,
        project: Option<&str>,
    ) -> Result<i64> {
        let now = Utc::now().timestamp();
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO pending_messages
               (session_id, status, tool_name, tool_input, tool_response, retry_count, created_at_epoch, project)
               VALUES ($1, 'pending', $2, $3, $4, 0, $5, $6)
               RETURNING id",
        )
        .bind(session_id)
        .bind(tool_name)
        .bind(tool_input)
        .bind(tool_response)
        .bind(now)
        .bind(project)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    async fn claim_pending_messages(
        &self,
        limit: usize,
        visibility_timeout_secs: i64,
    ) -> Result<Vec<PendingMessage>> {
        let now = Utc::now().timestamp();
        let stale_threshold = now - visibility_timeout_secs;
        let rows = sqlx::query(
            "UPDATE pending_messages
               SET status = 'processing', claimed_at_epoch = $1
               WHERE id IN (
                   SELECT id FROM pending_messages
                   WHERE status = 'pending'
                      OR (status = 'processing' AND claimed_at_epoch < $2)
                   ORDER BY created_at_epoch ASC
                   LIMIT $3
                   FOR UPDATE SKIP LOCKED
               )
               RETURNING id, session_id, status, tool_name, tool_input, tool_response,
                         retry_count, created_at_epoch, claimed_at_epoch, completed_at_epoch, project",
        )
        .bind(now)
        .bind(stale_threshold)
        .bind(usize_to_i64(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_pending_message).collect()
    }

    async fn complete_message(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM pending_messages WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn fail_message(&self, id: i64, increment_retry: bool) -> Result<()> {
        if increment_retry {
            sqlx::query(
                "UPDATE pending_messages
                   SET retry_count = retry_count + 1,
                       status = CASE
                           WHEN retry_count + 1 >= $1 THEN 'failed'
                           ELSE 'pending'
                       END,
                       claimed_at_epoch = NULL
                   WHERE id = $2",
            )
            .bind(max_retry_count())
            .bind(id)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query("UPDATE pending_messages SET status = 'failed' WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    async fn get_pending_count(&self) -> Result<usize> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pending_messages WHERE status = 'pending'")
                .fetch_one(&self.pool)
                .await?;
        Ok(usize::try_from(count).unwrap_or(0))
    }

    async fn release_stale_messages(&self, visibility_timeout_secs: i64) -> Result<usize> {
        let now = Utc::now().timestamp();
        let stale_threshold = now - visibility_timeout_secs;
        let result = sqlx::query(
            "UPDATE pending_messages
               SET status = 'pending', claimed_at_epoch = NULL
               WHERE status = 'processing' AND claimed_at_epoch <= $1",
        )
        .bind(stale_threshold)
        .execute(&self.pool)
        .await?;
        Ok(usize::try_from(result.rows_affected()).unwrap_or(usize::MAX))
    }

    async fn get_failed_messages(&self, limit: usize) -> Result<Vec<PendingMessage>> {
        let rows = sqlx::query(
            "SELECT id, session_id, status, tool_name, tool_input, tool_response,
                    retry_count, created_at_epoch, claimed_at_epoch, completed_at_epoch, project
               FROM pending_messages
               WHERE status = 'failed'
               ORDER BY created_at_epoch DESC
               LIMIT $1",
        )
        .bind(usize_to_i64(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_pending_message).collect()
    }

    async fn get_all_pending_messages(&self, limit: usize) -> Result<Vec<PendingMessage>> {
        let rows = sqlx::query(
            "SELECT id, session_id, status, tool_name, tool_input, tool_response,
                    retry_count, created_at_epoch, claimed_at_epoch, completed_at_epoch, project
               FROM pending_messages
               ORDER BY created_at_epoch DESC
               LIMIT $1",
        )
        .bind(usize_to_i64(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_pending_message).collect()
    }

    async fn get_queue_stats(&self) -> Result<QueueStats> {
        let row = sqlx::query(
            "SELECT
               COALESCE(SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END), 0) as pending,
               COALESCE(SUM(CASE WHEN status = 'processing' THEN 1 ELSE 0 END), 0) as processing,
               COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) as failed,
               COALESCE(SUM(CASE WHEN status = 'processed' THEN 1 ELSE 0 END), 0) as processed
             FROM pending_messages",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(QueueStats {
            pending: u64::try_from(row.try_get::<i64, _>("pending")?).unwrap_or(0),
            processing: u64::try_from(row.try_get::<i64, _>("processing")?).unwrap_or(0),
            failed: u64::try_from(row.try_get::<i64, _>("failed")?).unwrap_or(0),
            processed: u64::try_from(row.try_get::<i64, _>("processed")?).unwrap_or(0),
        })
    }

    async fn clear_failed_messages(&self) -> Result<usize> {
        let result = sqlx::query("DELETE FROM pending_messages WHERE status = 'failed'")
            .execute(&self.pool)
            .await?;
        Ok(usize::try_from(result.rows_affected()).unwrap_or(usize::MAX))
    }

    async fn retry_failed_messages(&self) -> Result<usize> {
        let result = sqlx::query(
            "UPDATE pending_messages
               SET status = 'pending', retry_count = 0, claimed_at_epoch = NULL
               WHERE status = 'failed'",
        )
        .execute(&self.pool)
        .await?;
        Ok(usize::try_from(result.rows_affected()).unwrap_or(usize::MAX))
    }

    async fn clear_all_pending_messages(&self) -> Result<usize> {
        let result = sqlx::query("DELETE FROM pending_messages").execute(&self.pool).await?;
        Ok(usize::try_from(result.rows_affected()).unwrap_or(usize::MAX))
    }
}
