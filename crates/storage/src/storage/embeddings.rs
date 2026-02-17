use anyhow::Result;
use opencode_mem_core::{is_zero_vector, Observation, SimilarMatch, EMBEDDING_DIMENSION};
use rusqlite::params;
use zerocopy::IntoBytes;

use super::{coerce_to_sql, get_conn, log_row_error, Storage};

impl Storage {
    /// Stores an embedding vector for an observation.
    ///
    /// # Errors
    /// Returns error if database operation fails.
    pub fn store_embedding(&self, observation_id: &str, embedding: &[f32]) -> Result<()> {
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
        let conn = get_conn(&self.pool)?;
        let rowid: i64 = conn.query_row(
            "SELECT rowid FROM observations WHERE id = ?1",
            params![observation_id],
            |row| row.get(0),
        )?;
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM observations_vec WHERE rowid = ?1", params![rowid])?;
        tx.execute(
            "INSERT INTO observations_vec(rowid, embedding) VALUES (?1, ?2)",
            params![rowid, embedding.as_bytes()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Drops and recreates the `observations_vec` virtual table,
    /// causing all observations to be re-embedded by the background loop.
    ///
    /// # Errors
    /// Returns error if database operation fails.
    pub fn clear_embeddings(&self) -> Result<()> {
        let conn = get_conn(&self.pool)?;
        conn.execute_batch(&format!(
            "DROP TABLE IF EXISTS observations_vec;
             CREATE VIRTUAL TABLE IF NOT EXISTS observations_vec USING vec0(
                 embedding float[{EMBEDDING_DIMENSION}] distance_metric=cosine
             );",
        ))?;
        Ok(())
    }

    /// Returns observations that don't have embeddings yet.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_observations_without_embeddings(&self, limit: usize) -> Result<Vec<Observation>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT o.id, o.session_id, o.project, o.observation_type, o.title, o.subtitle,
                  o.narrative, o.facts, o.concepts, o.files_read, o.files_modified,
                  o.keywords, o.prompt_number, o.discovery_tokens,
                  o.noise_level, o.noise_reason, o.created_at
           FROM observations o
           LEFT JOIN observations_vec v ON o.rowid = v.rowid
           WHERE v.rowid IS NULL
           LIMIT ?1",
        )?;
        let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let results = stmt
            .query_map(params![limit_i64], Self::row_to_observation)?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub fn find_similar(&self, embedding: &[f32], threshold: f32) -> Result<Option<SimilarMatch>> {
        if embedding.is_empty() || is_zero_vector(embedding) {
            return Ok(None);
        }

        let conn = get_conn(&self.pool)?;

        let vec_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM observations_vec", [], |row| row.get(0))
            .unwrap_or(0);
        if vec_count == 0 {
            return Ok(None);
        }

        let query_bytes = embedding.as_bytes();

        let stmt_result = conn.prepare(
            "SELECT o.id, o.title,
                    (1.0 - vec_distance_cosine(v.embedding, ?1)) as similarity
               FROM observations_vec v
               JOIN observations o ON o.rowid = v.rowid
               ORDER BY vec_distance_cosine(v.embedding, ?1)
               LIMIT 1",
        );

        let mut stmt = match stmt_result {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    "Vector search unavailable for dedup (sqlite-vec not loaded?): {}",
                    e
                );
                return Ok(None);
            },
        };

        let result = stmt.query_row(params![query_bytes], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let similarity: f64 = row.get(2)?;
            Ok((id, title, similarity))
        });

        match result {
            Ok((id, title, similarity)) => {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "similarity score f64→f32 is acceptable lossy narrowing"
                )]
                let sim_f32 = similarity as f32;
                if sim_f32 >= threshold {
                    Ok(Some(SimilarMatch { observation_id: id, similarity: sim_f32, title }))
                } else {
                    Ok(None)
                }
            },
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => {
                tracing::warn!("find_similar query failed: {}", e);
                Ok(None)
            },
        }
    }

    pub fn find_similar_many(
        &self,
        embedding: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Result<Vec<SimilarMatch>> {
        if embedding.is_empty() || is_zero_vector(embedding) {
            return Ok(Vec::new());
        }

        let conn = get_conn(&self.pool)?;

        let vec_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM observations_vec", [], |row| row.get(0))
            .unwrap_or(0);
        if vec_count == 0 {
            return Ok(Vec::new());
        }

        let query_bytes = embedding.as_bytes();
        #[expect(
            clippy::cast_possible_wrap,
            reason = "limit is always small (≤100), safe usize→i64"
        )]
        let limit_i64 = limit as i64;

        let stmt_result = conn.prepare(
            "SELECT o.id, o.title,
                    (1.0 - vec_distance_cosine(v.embedding, ?1)) as similarity
               FROM observations_vec v
               JOIN observations o ON o.rowid = v.rowid
               ORDER BY vec_distance_cosine(v.embedding, ?1)
               LIMIT ?2",
        );

        let mut stmt = match stmt_result {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    "Vector search unavailable for find_similar_many (sqlite-vec not loaded?): {}",
                    e
                );
                return Ok(Vec::new());
            },
        };

        let rows = stmt.query_map(params![query_bytes, limit_i64], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let similarity: f64 = row.get(2)?;
            Ok((id, title, similarity))
        })?;

        let mut matches = Vec::new();
        for row in rows {
            match row {
                Ok((id, title, similarity)) => {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "similarity score f64→f32 is acceptable lossy narrowing"
                    )]
                    let sim_f32 = similarity as f32;
                    if sim_f32 >= threshold {
                        matches.push(SimilarMatch {
                            observation_id: id,
                            similarity: sim_f32,
                            title,
                        });
                    }
                },
                Err(e) => {
                    tracing::warn!("find_similar_many row error: {}", e);
                },
            }
        }

        Ok(matches)
    }

    pub fn get_embeddings_for_ids(&self, ids: &[String]) -> Result<Vec<(String, Vec<f32>)>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = get_conn(&self.pool)?;
        let mut results = Vec::new();
        for chunk in ids.chunks(opencode_mem_core::MAX_BATCH_IDS) {
            let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT o.id, v.embedding
                   FROM observations o
                   JOIN observations_vec v ON o.rowid = v.rowid
                  WHERE o.id IN ({placeholders})"
            );
            let stmt_result = conn.prepare(&sql);
            let mut stmt = match stmt_result {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("Vector table unavailable for get_embeddings_for_ids: {}", e);
                    return Ok(Vec::new());
                },
            };
            let sql_params: Vec<&dyn rusqlite::ToSql> = chunk.iter().map(coerce_to_sql).collect();
            let rows = stmt.query_map(sql_params.as_slice(), |row| {
                let id: String = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                Ok((id, blob))
            })?;

            for row in rows {
                match row {
                    Ok((id, blob)) => {
                        let floats = blob_to_f32_vec(&blob);
                        results.push((id, floats));
                    },
                    Err(e) => {
                        tracing::warn!("get_embeddings_for_ids row error: {}", e);
                    },
                }
            }
        }
        Ok(results)
    }
}

fn blob_to_f32_vec(blob: &[u8]) -> Vec<f32> {
    if !blob.len().is_multiple_of(4) {
        tracing::warn!(
            blob_len = blob.len(),
            "Embedding blob has non-aligned length — trailing bytes will be dropped"
        );
    }
    blob.chunks_exact(4)
        .map(|chunk| {
            let mut arr = [0u8; 4];
            arr.copy_from_slice(chunk);
            f32::from_le_bytes(arr)
        })
        .collect()
}
