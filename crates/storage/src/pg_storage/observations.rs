//! ObservationStore implementation for PgStorage.

use super::*;

use crate::error::StorageError;
use crate::traits::ObservationStore;
use async_trait::async_trait;

const OBSERVATION_COLUMNS: &str = "id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, files_read, files_modified, keywords, prompt_number, discovery_tokens, noise_level, noise_reason, created_at";

#[async_trait]
impl ObservationStore for PgStorage {
    async fn save_observation(&self, obs: &Observation) -> Result<bool, StorageError> {
        let result = sqlx::query(
            r#"INSERT INTO observations
               (id, session_id, project, observation_type, title, subtitle, narrative,
                facts, concepts, files_read, files_modified, keywords,
                prompt_number, discovery_tokens, noise_level, noise_reason, created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
               ON CONFLICT (id) DO NOTHING"#,
        )
        .bind(&obs.id)
        .bind(&obs.session_id)
        .bind(&obs.project)
        .bind(obs.observation_type.as_str())
        .bind(&obs.title)
        .bind(&obs.subtitle)
        .bind(&obs.narrative)
        .bind(serde_json::to_value(&obs.facts)?)
        .bind(serde_json::to_value(&obs.concepts)?)
        .bind(serde_json::to_value(&obs.files_read)?)
        .bind(serde_json::to_value(&obs.files_modified)?)
        .bind(serde_json::to_value(&obs.keywords)?)
        .bind(obs.prompt_number.map(|v| v.as_pg_i32()).transpose().map_err(|e| {
            StorageError::DataCorruption {
                context: "prompt_number exceeds i32::MAX".into(),
                source: Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()),
            }
        })?)
        .bind(obs.discovery_tokens.map(|v| v.as_pg_i32()).transpose().map_err(|e| {
            StorageError::DataCorruption {
                context: "discovery_tokens exceeds i32::MAX".into(),
                source: Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()),
            }
        })?)
        .bind(obs.noise_level.as_str())
        .bind(&obs.noise_reason)
        .bind(obs.created_at)
        .execute(&self.pool)
        .await;
        match result {
            Ok(r) => Ok(r.rows_affected() > 0),
            Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23505") => {
                tracing::debug!(
                    id = %obs.id,
                    title = %obs.title,
                    "Observation already exists (duplicate id or title), skipping"
                );
                Ok(false)
            },
            Err(e) => Err(e.into()),
        }
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<Observation>, StorageError> {
        let row = sqlx::query(&format!(
            "SELECT {}
             FROM observations WHERE id = $1",
            OBSERVATION_COLUMNS
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| row_to_observation(&r)).transpose()
    }

    async fn get_recent(&self, limit: usize) -> Result<Vec<Observation>, StorageError> {
        let rows = sqlx::query(&format!(
            "SELECT {}
             FROM observations ORDER BY created_at DESC LIMIT $1",
            OBSERVATION_COLUMNS
        ))
        .bind(usize_to_i64(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_observation).collect()
    }

    async fn get_session_observations(
        &self,
        session_id: &str,
    ) -> Result<Vec<Observation>, StorageError> {
        let rows = sqlx::query(&format!(
            "SELECT {}
             FROM observations WHERE session_id = $1 ORDER BY created_at",
            OBSERVATION_COLUMNS
        ))
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_observation).collect()
    }

    async fn get_observations_by_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<Observation>, StorageError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(&format!(
            "SELECT {}
             FROM observations WHERE id = ANY($1) ORDER BY created_at DESC",
            OBSERVATION_COLUMNS
        ))
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_observation).collect()
    }

    async fn get_context_for_project(
        &self,
        project: &str,
        limit: usize,
    ) -> Result<Vec<Observation>, StorageError> {
        let rows = sqlx::query(&format!(
            "SELECT {}
             FROM observations
             WHERE project = $1
               AND noise_level NOT IN ('low', 'negligible')
             ORDER BY created_at DESC LIMIT $2",
            OBSERVATION_COLUMNS
        ))
        .bind(project)
        .bind(usize_to_i64(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_observation).collect()
    }

    async fn get_session_observation_count(&self, session_id: &str) -> Result<usize, StorageError> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM observations WHERE session_id = $1")
                .bind(session_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(usize::try_from(count).unwrap_or(0))
    }

    async fn search_by_file(
        &self,
        file_path: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        let jsonb_str = serde_json::json!([file_path]).to_string();
        let rows = sqlx::query(
            r#"SELECT id, title, subtitle, observation_type, noise_level, 0.0::float8 as score
               FROM observations
               WHERE files_read @> $1::jsonb OR files_modified @> $1::jsonb
               ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(&jsonb_str)
        .bind(usize_to_i64(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_search_result).collect()
    }

    async fn merge_into_existing(
        &self,
        existing_id: &str,
        newer: &Observation,
    ) -> Result<(), StorageError> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query(&format!(
            "SELECT {}
             FROM observations WHERE id = $1 FOR UPDATE",
            OBSERVATION_COLUMNS
        ))
        .bind(existing_id)
        .fetch_optional(&mut *tx)
        .await?;

        let existing = row.map(|r| row_to_observation(&r)).transpose()?.ok_or_else(|| {
            StorageError::NotFound { entity: "observation", id: existing_id.to_owned() }
        })?;

        let merged = opencode_mem_core::compute_merge(&existing, newer);

        sqlx::query(
            "UPDATE observations SET facts = $1, keywords = $2, files_read = $3,
                    files_modified = $4, narrative = $5, created_at = $6, concepts = $7,
                    noise_level = $8, subtitle = $9, noise_reason = $10,
                    prompt_number = $11, discovery_tokens = $12, title = $14, observation_type = $15
               WHERE id = $13",
        )
        .bind(serde_json::to_value(&merged.facts)?)
        .bind(serde_json::to_value(&merged.keywords)?)
        .bind(serde_json::to_value(&merged.files_read)?)
        .bind(serde_json::to_value(&merged.files_modified)?)
        .bind(&merged.narrative)
        .bind(merged.created_at)
        .bind(serde_json::to_value(&merged.concepts)?)
        .bind(merged.noise_level.as_str())
        .bind(&merged.subtitle)
        .bind(&merged.noise_reason)
        .bind(merged.prompt_number.map(|v| v.as_pg_i32()).transpose().map_err(|e| {
            StorageError::DataCorruption {
                context: "prompt_number exceeds i32::MAX".into(),
                source: Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()),
            }
        })?)
        .bind(merged.discovery_tokens.map(|v| v.as_pg_i32()).transpose().map_err(|e| {
            StorageError::DataCorruption {
                context: "discovery_tokens exceeds i32::MAX".into(),
                source: Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()),
            }
        })?)
        .bind(existing_id)
        .bind(&merged.title)
        .bind(merged.observation_type.as_str())
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }
}

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
}
