//! EmbeddingStore implementation for PgStorage.

use super::*;

use crate::traits::EmbeddingStore;
use async_trait::async_trait;
use opencode_mem_core::{is_zero_vector, SimilarMatch, EMBEDDING_DIMENSION};

#[async_trait]
impl EmbeddingStore for PgStorage {
    async fn store_embedding(&self, observation_id: &str, embedding: &[f32]) -> Result<()> {
        if embedding.len() != EMBEDDING_DIMENSION {
            anyhow::bail!(
                "embedding dimension mismatch: expected {EMBEDDING_DIMENSION}, got {}",
                embedding.len()
            );
        }
        if is_zero_vector(embedding) {
            tracing::warn!(
                observation_id,
                "Rejecting zero vector embedding (would produce NaN in cosine distance)"
            );
            return Ok(());
        }
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
        .bind(usize_to_i64(limit))
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
        if embedding.is_empty() || is_zero_vector(embedding) {
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
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "similarity score f64→f32 is acceptable lossy narrowing"
                )]
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

    async fn find_similar_many(
        &self,
        embedding: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Result<Vec<SimilarMatch>> {
        if embedding.is_empty() || is_zero_vector(embedding) {
            return Ok(Vec::new());
        }

        let vec_str =
            format!("[{}]", embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));

        let rows = sqlx::query(
            "SELECT id, title, 1.0 - (embedding <=> $1::vector) AS similarity
               FROM observations
              WHERE embedding IS NOT NULL
              ORDER BY embedding <=> $1::vector
              LIMIT $2",
        )
        .bind(&vec_str)
        .bind(usize_to_i64(limit))
        .fetch_all(&self.pool)
        .await?;

        let mut matches = Vec::new();
        for r in &rows {
            let similarity: f64 = r.try_get("similarity")?;
            #[allow(
                clippy::cast_possible_truncation,
                reason = "similarity score f64→f32 is acceptable lossy narrowing"
            )]
            let sim_f32 = similarity as f32;
            if sim_f32 >= threshold {
                matches.push(SimilarMatch {
                    observation_id: r.try_get("id")?,
                    similarity: sim_f32,
                    title: r.try_get("title")?,
                });
            }
        }

        Ok(matches)
    }

    async fn get_embeddings_for_ids(&self, ids: &[String]) -> Result<Vec<(String, Vec<f32>)>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query(
            "SELECT id, embedding::text
               FROM observations
              WHERE id = ANY($1) AND embedding IS NOT NULL",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::new();
        for r in &rows {
            let id: String = r.try_get("id")?;
            let emb_text: String = r.try_get("embedding")?;
            let floats = parse_pg_vector_text(&emb_text);
            results.push((id, floats));
        }
        Ok(results)
    }
}

fn parse_pg_vector_text(s: &str) -> Vec<f32> {
    let trimmed = s.trim_start_matches('[').trim_end_matches(']');
    let result: Vec<f32> =
        trimmed.split(',').filter_map(|v| v.trim().parse::<f32>().ok()).collect();
    let expected = opencode_mem_core::EMBEDDING_DIMENSION;
    if !result.is_empty() && result.len() != expected {
        tracing::warn!(
            parsed = result.len(),
            expected,
            "Parsed PG vector has unexpected dimension — cosine similarity will return 0.0"
        );
    }
    result
}
