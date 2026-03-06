//! Delete/cleanup operations for observations on PgStorage.

use super::*;

use crate::error::StorageError;

impl PgStorage {
    /// Delete an observation by ID. Returns `true` if a row was deleted.
    ///
    /// Used by background dedup sweep to remove duplicate observations after merge.
    /// Not part of the `ObservationStore` trait — only available on the concrete backend.
    pub async fn delete_observation_by_id(&self, id: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM observations WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Transactional cascading delete: unlinks from knowledge `source_observations`, then deletes.
    pub async fn delete_observation_cascading(&self, id: &str) -> Result<bool, StorageError> {
        let mut tx = self.pool.begin().await?;

        let obs_json = serde_json::json!([id]).to_string();
        sqlx::query(
            r#"UPDATE global_knowledge
               SET source_observations = (source_observations - $1::jsonb)
               WHERE source_observations @> $1::jsonb"#,
        )
        .bind(&obs_json)
        .execute(&mut *tx)
        .await?;

        let result = sqlx::query("DELETE FROM observations WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }
}
