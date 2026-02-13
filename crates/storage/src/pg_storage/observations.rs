//! ObservationStore implementation for PgStorage.

use super::*;

use crate::traits::ObservationStore;
use async_trait::async_trait;

#[async_trait]
impl ObservationStore for PgStorage {
    async fn save_observation(&self, obs: &Observation) -> Result<bool> {
        let result = match sqlx::query(
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
        .bind(obs.prompt_number.map(|v| v as i32))
        .bind(obs.discovery_tokens.map(|v| v as i32))
        .bind(obs.noise_level.as_str())
        .bind(&obs.noise_reason)
        .bind(obs.created_at)
        .execute(&self.pool)
        .await
        {
            Ok(r) => r,
            Err(sqlx::Error::Database(ref db_err)) if db_err.code().as_deref() == Some("23505") => {
                return Ok(false);
            },
            Err(e) => return Err(e.into()),
        };
        Ok(result.rows_affected() > 0)
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

    async fn get_recent(&self, limit: usize) -> Result<Vec<SearchResult>> {
        let rows = sqlx::query(
            "SELECT id, title, subtitle, observation_type, noise_level, 1.0::float8 as score
             FROM observations ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_search_result).collect()
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
        .bind(limit as i64)
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
        Ok(count as usize)
    }

    async fn search_by_file(&self, file_path: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let pattern = format!("%{}%", escape_like(file_path));
        let rows = sqlx::query(
            r#"SELECT id, title, subtitle, observation_type, noise_level, 1.0::float8 as score
               FROM observations
               WHERE files_read::text LIKE $1 OR files_modified::text LIKE $1
               ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(&pattern)
        .bind(limit as i64)
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

        let facts = union_dedup(&existing.facts, &newer.facts);
        let keywords = union_dedup(&existing.keywords, &newer.keywords);
        let files_read = union_dedup(&existing.files_read, &newer.files_read);
        let files_modified = union_dedup(&existing.files_modified, &newer.files_modified);
        let concepts = union_dedup_concepts(&existing.concepts, &newer.concepts);

        let narrative: Option<&str> = match (&existing.narrative, &newer.narrative) {
            (Some(e), Some(n)) if n.len() > e.len() => Some(n.as_str()),
            (None, Some(n)) => Some(n.as_str()),
            (Some(e), _) => Some(e.as_str()),
            (None, None) => None,
        };

        let created_at = existing.created_at.max(newer.created_at);

        sqlx::query(
            "UPDATE observations SET facts = $1, keywords = $2, files_read = $3,
                    files_modified = $4, narrative = $5, created_at = $6, concepts = $7
               WHERE id = $8",
        )
        .bind(serde_json::to_value(&facts)?)
        .bind(serde_json::to_value(&keywords)?)
        .bind(serde_json::to_value(&files_read)?)
        .bind(serde_json::to_value(&files_modified)?)
        .bind(narrative)
        .bind(created_at)
        .bind(serde_json::to_value(&concepts)?)
        .bind(existing_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }
}
