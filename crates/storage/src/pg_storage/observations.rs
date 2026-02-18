//! ObservationStore implementation for PgStorage.

use super::*;

use crate::traits::ObservationStore;
use anyhow::Context;
use async_trait::async_trait;

#[async_trait]
impl ObservationStore for PgStorage {
    async fn save_observation(&self, obs: &Observation) -> Result<bool> {
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
        .bind(
            obs.prompt_number
                .map(|v| i32::try_from(v.0))
                .transpose()
                .context("prompt_number exceeds i32::MAX")?,
        )
        .bind(
            obs.discovery_tokens
                .map(|v| i32::try_from(v.0))
                .transpose()
                .context("discovery_tokens exceeds i32::MAX")?,
        )
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

    async fn get_by_id(&self, id: &str) -> Result<Option<Observation>> {
        let row = sqlx::query(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                    facts, concepts, files_read, files_modified, keywords,
                    prompt_number, discovery_tokens, noise_level, noise_reason, created_at
             FROM observations WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| row_to_observation(&r)).transpose()
    }

    async fn get_recent(&self, limit: usize) -> Result<Vec<Observation>> {
        let rows = sqlx::query(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                    facts, concepts, files_read, files_modified, keywords,
                    prompt_number, discovery_tokens, noise_level, noise_reason, created_at
             FROM observations ORDER BY created_at DESC LIMIT $1",
        )
        .bind(usize_to_i64(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_observation).collect()
    }

    async fn get_session_observations(&self, session_id: &str) -> Result<Vec<Observation>> {
        let rows = sqlx::query(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                    facts, concepts, files_read, files_modified, keywords,
                    prompt_number, discovery_tokens, noise_level, noise_reason, created_at
             FROM observations WHERE session_id = $1 ORDER BY created_at",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_observation).collect()
    }

    async fn get_observations_by_ids(&self, ids: &[String]) -> Result<Vec<Observation>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                    facts, concepts, files_read, files_modified, keywords,
                    prompt_number, discovery_tokens, noise_level, noise_reason, created_at
             FROM observations WHERE id = ANY($1) ORDER BY created_at DESC",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_observation).collect()
    }

    async fn get_context_for_project(
        &self,
        project: &str,
        limit: usize,
    ) -> Result<Vec<Observation>> {
        let rows = sqlx::query(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                    facts, concepts, files_read, files_modified, keywords,
                    prompt_number, discovery_tokens, noise_level, noise_reason, created_at
             FROM observations WHERE project = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(project)
        .bind(usize_to_i64(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_observation).collect()
    }

    async fn get_session_observation_count(&self, session_id: &str) -> Result<usize> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM observations WHERE session_id = $1")
                .bind(session_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(usize::try_from(count).unwrap_or(0))
    }

    async fn search_by_file(&self, file_path: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let pattern = format!("%{}%", escape_like(file_path));
        let rows = sqlx::query(
            r#"SELECT id, title, subtitle, observation_type, noise_level, 0.0::float8 as score
               FROM observations
               WHERE EXISTS (SELECT 1 FROM jsonb_array_elements_text(files_read) AS f WHERE f LIKE $1)
                  OR EXISTS (SELECT 1 FROM jsonb_array_elements_text(files_modified) AS f WHERE f LIKE $1)
               ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(&pattern)
        .bind(usize_to_i64(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_search_result).collect()
    }

    async fn merge_into_existing(&self, existing_id: &str, newer: &Observation) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                    facts, concepts, files_read, files_modified, keywords,
                    prompt_number, discovery_tokens, noise_level, noise_reason, created_at
             FROM observations WHERE id = $1 FOR UPDATE",
        )
        .bind(existing_id)
        .fetch_optional(&mut *tx)
        .await?;

        let existing = row
            .map(|r| row_to_observation(&r))
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("observation not found: {existing_id}"))?;

        let merged = opencode_mem_core::compute_merge(&existing, newer);

        sqlx::query(
            "UPDATE observations SET facts = $1, keywords = $2, files_read = $3,
                    files_modified = $4, narrative = $5, created_at = $6, concepts = $7,
                    noise_level = $8, subtitle = $9, noise_reason = $10,
                    prompt_number = $11, discovery_tokens = $12
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
        .bind(
            merged
                .prompt_number
                .map(|v| i32::try_from(v.0))
                .transpose()
                .context("prompt_number exceeds i32::MAX")?,
        )
        .bind(
            merged
                .discovery_tokens
                .map(|v| i32::try_from(v.0))
                .transpose()
                .context("discovery_tokens exceeds i32::MAX")?,
        )
        .bind(existing_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }
}
