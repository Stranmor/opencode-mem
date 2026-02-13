//! EmbeddingStore implementation for PgStorage.

use super::*;

use crate::traits::EmbeddingStore;
use async_trait::async_trait;
use opencode_mem_core::SimilarMatch;

#[async_trait]
impl EmbeddingStore for PgStorage {
    async fn store_embedding(&self, observation_id: &str, embedding: &[f32]) -> Result<()> {
        let vec_str =
            format!("[{}]", embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));
        sqlx::query("UPDATE observations SET embedding = $1::vector WHERE id = $2")
            .bind(&vec_str)
            .bind(observation_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_observations_without_embeddings(&self, limit: usize) -> Result<Vec<Observation>> {
        let rows = sqlx::query(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                    facts, concepts, files_read, files_modified, keywords, prompt_number,
                    discovery_tokens, noise_level, noise_reason, created_at
               FROM observations
               WHERE embedding IS NULL
               LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_observation).collect()
    }

    async fn clear_embeddings(&self) -> Result<()> {
        sqlx::query("UPDATE observations SET embedding = NULL").execute(&self.pool).await?;
        Ok(())
    }

    async fn find_similar(
        &self,
        embedding: &[f32],
        threshold: f32,
    ) -> Result<Option<SimilarMatch>> {
        if embedding.is_empty() {
            return Ok(None);
        }

        let has_embeddings: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM observations WHERE embedding IS NOT NULL")
                .fetch_one(&self.pool)
                .await?;
        if has_embeddings == 0 {
            return Ok(None);
        }

        let vec_str =
            format!("[{}]", embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));

        let row = sqlx::query(
            "SELECT id, title, 1.0 - (embedding <=> $1::vector) AS similarity
               FROM observations
              WHERE embedding IS NOT NULL
              ORDER BY embedding <=> $1::vector
              LIMIT 1",
        )
        .bind(&vec_str)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => {
                let similarity: f64 = r.try_get("similarity")?;
                let sim_f32 = similarity as f32;
                if sim_f32 >= threshold {
                    Ok(Some(SimilarMatch {
                        observation_id: r.try_get("id")?,
                        similarity: sim_f32,
                        title: r.try_get("title")?,
                    }))
                } else {
                    Ok(None)
                }
            },
            None => Ok(None),
        }
    }
}
