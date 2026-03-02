//! EmbeddingStore implementation for PgStorage.

use pgvector::Vector;

use super::*;

use crate::error::StorageError;
use crate::traits::EmbeddingStore;
use async_trait::async_trait;
use opencode_mem_core::{
    contains_non_finite, is_zero_vector, SimilarMatch, EMBEDDING_DIMENSION, MAX_BATCH_IDS,
};

#[async_trait]
impl EmbeddingStore for PgStorage {
    async fn store_embedding(
        &self,
        observation_id: &str,
        embedding: &[f32],
    ) -> Result<(), StorageError> {
        if embedding.len() != EMBEDDING_DIMENSION {
            return Err(StorageError::DataCorruption {
                context: format!(
                    "embedding dimension mismatch: expected {EMBEDDING_DIMENSION}, got {}",
                    embedding.len()
                ),
                source: "dimension check".into(),
            });
        }
        if is_zero_vector(embedding) {
            tracing::warn!(
                observation_id,
                "Rejecting zero vector embedding (would produce NaN in cosine distance)"
            );
            return Ok(());
        }
        if contains_non_finite(embedding) {
            return Err(StorageError::DataCorruption {
                context: "embedding contains NaN or Infinity values".to_owned(),
                source: Box::from("non-finite check"),
            });
        }

        let vector = Vector::from(embedding.to_vec());
        sqlx::query("UPDATE observations SET embedding = $1 WHERE id = $2")
            .bind(vector)
            .bind(observation_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_observations_without_embeddings(
        &self,
        limit: usize,
    ) -> Result<Vec<Observation>, StorageError> {
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

    async fn clear_embeddings(&self) -> Result<(), StorageError> {
        sqlx::query("UPDATE observations SET embedding = NULL").execute(&self.pool).await?;
        Ok(())
    }

    async fn find_similar(
        &self,
        embedding: &[f32],
        threshold: f32,
        project: Option<&str>,
    ) -> Result<Option<SimilarMatch>, StorageError> {
        if embedding.is_empty() || is_zero_vector(embedding) || contains_non_finite(embedding) {
            return Ok(None);
        }

        let vector = Vector::from(embedding.to_vec());

        let row = sqlx::query(
            "SELECT id, title, 1.0 - (embedding <=> $1) AS similarity
               FROM observations
              WHERE embedding IS NOT NULL
                AND (project = $2 OR $2 IS NULL)
              ORDER BY embedding <=> $1
              LIMIT 1",
        )
        .bind(vector)
        .bind(project)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => {
                let similarity: f64 = r.try_get("similarity")?;
                #[expect(
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
        project: Option<&str>,
    ) -> Result<Vec<SimilarMatch>, StorageError> {
        if embedding.is_empty() || is_zero_vector(embedding) || contains_non_finite(embedding) {
            return Ok(Vec::new());
        }

        let vector = Vector::from(embedding.to_vec());

        let rows = sqlx::query(
            "SELECT id, title, 1.0 - (embedding <=> $1) AS similarity
               FROM observations
              WHERE embedding IS NOT NULL
                AND (project = $3 OR $3 IS NULL)
              ORDER BY embedding <=> $1
              LIMIT $2",
        )
        .bind(vector)
        .bind(usize_to_i64(limit))
        .bind(project)
        .fetch_all(&self.pool)
        .await?;

        let mut matches = Vec::new();
        for r in &rows {
            let similarity: f64 = r.try_get("similarity")?;
            #[expect(
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

    async fn get_embeddings_for_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<(String, Vec<f32>)>, StorageError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_results = Vec::new();
        for chunk in ids.chunks(MAX_BATCH_IDS) {
            let chunk_vec: Vec<String> = chunk.to_vec();
            let rows = sqlx::query(
                "SELECT id, embedding
                   FROM observations
                  WHERE id = ANY($1) AND embedding IS NOT NULL",
            )
            .bind(&chunk_vec)
            .fetch_all(&self.pool)
            .await?;

            for r in &rows {
                let id: String = r.try_get("id")?;
                let vector: Vector = r.try_get("embedding")?;
                all_results.push((id, vector.to_vec()));
            }
        }
        Ok(all_results)
    }
}
