//! SessionStore implementation for PgStorage.

use super::*;

use crate::error::StorageError;
use crate::traits::SessionStore;
use async_trait::async_trait;

#[async_trait]
impl SessionStore for PgStorage {
    async fn save_session(&self, session: &Session) -> Result<(), StorageError> {
        sqlx::query(&format!(
            "INSERT INTO sessions ({SESSION_COLUMNS})
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (id) DO UPDATE SET
               content_session_id = EXCLUDED.content_session_id,
               memory_session_id = EXCLUDED.memory_session_id,
               project = EXCLUDED.project,
               user_prompt = EXCLUDED.user_prompt,
               started_at = EXCLUDED.started_at,
               ended_at = EXCLUDED.ended_at,
               status = EXCLUDED.status,
               prompt_counter = EXCLUDED.prompt_counter"
        ))
        .bind(&session.id)
        .bind(&session.content_session_id)
        .bind(&session.memory_session_id)
        .bind(&session.project)
        .bind(&session.user_prompt)
        .bind(session.started_at)
        .bind(session.ended_at)
        .bind(session.status.as_str())
        .bind(i32::try_from(session.prompt_counter).map_err(|e| StorageError::DataCorruption {
            context: "prompt_counter exceeds i32::MAX".into(),
            source: Box::new(e),
        })?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_session(&self, id: &str) -> Result<Option<Session>, StorageError> {
        let row = sqlx::query(&format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|r| row_to_session(&r)).transpose()
    }

    async fn get_session_by_content_id(
        &self,
        content_session_id: &str,
    ) -> Result<Option<Session>, StorageError> {
        let row = sqlx::query(&format!(
            "SELECT {SESSION_COLUMNS} FROM sessions WHERE content_session_id = $1 ORDER BY started_at DESC LIMIT 1"
        ))
        .bind(content_session_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| row_to_session(&r)).transpose()
    }

    async fn update_session_status(
        &self,
        id: &str,
        status: SessionStatus,
    ) -> Result<(), StorageError> {
        let ended_at: Option<DateTime<Utc>> = (status != SessionStatus::Active).then(Utc::now);
        sqlx::query("UPDATE sessions SET status = $1, ended_at = $2 WHERE id = $3")
            .bind(status.as_str())
            .bind(ended_at)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_session(&self, session_id: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn close_stale_sessions(&self, max_age_hours: i64) -> Result<usize, StorageError> {
        let now = Utc::now();
        let threshold = now - chrono::Duration::hours(max_age_hours);
        let result = sqlx::query(
            "UPDATE sessions SET status = $1, ended_at = $2
             WHERE status = $3 AND started_at < $4",
        )
        .bind(SessionStatus::Completed.as_str())
        .bind(now)
        .bind(SessionStatus::Active.as_str())
        .bind(threshold)
        .execute(&self.pool)
        .await?;
        Ok(usize::try_from(result.rows_affected()).unwrap_or(usize::MAX))
    }
}
