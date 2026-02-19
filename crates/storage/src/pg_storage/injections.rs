use super::*;

use crate::error::StorageError;
use crate::traits::InjectionStore;
use async_trait::async_trait;

#[async_trait]
impl InjectionStore for PgStorage {
    async fn save_injected_observations(
        &self,
        session_id: &str,
        observation_ids: &[String],
    ) -> Result<(), StorageError> {
        if observation_ids.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for obs_id in observation_ids {
            sqlx::query(
                "INSERT INTO injected_observations (session_id, observation_id)
                 VALUES ($1, $2)
                 ON CONFLICT (session_id, observation_id) DO NOTHING",
            )
            .bind(session_id)
            .bind(obs_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn get_injected_observation_ids(
        &self,
        session_id: &str,
    ) -> Result<Vec<String>, StorageError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT observation_id FROM injected_observations WHERE session_id = $1",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    async fn cleanup_old_injections(&self, older_than_hours: u32) -> Result<u64, StorageError> {
        let interval = format!("{older_than_hours} hours");
        let result = sqlx::query(
            "DELETE FROM injected_observations WHERE injected_at < NOW() - $1::interval",
        )
        .bind(&interval)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}
