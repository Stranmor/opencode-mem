use anyhow::Result;
use opencode_mem_core::{Observation, SimilarMatch};
use rusqlite::params;
use zerocopy::IntoBytes;

use super::{get_conn, log_row_error, Storage};

impl Storage {
    /// Stores an embedding vector for an observation.
    ///
    /// # Errors
    /// Returns error if database operation fails.
    pub fn store_embedding(&self, observation_id: &str, embedding: &[f32]) -> Result<()> {
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
        conn.execute_batch(
            "DROP TABLE IF EXISTS observations_vec;
             CREATE VIRTUAL TABLE IF NOT EXISTS observations_vec USING vec0(
                 embedding float[384] distance_metric=cosine
             );",
        )?;
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
        let limit_i64 = limit as i64;
        let results = stmt
            .query_map(params![limit_i64], Self::row_to_observation)?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub fn find_similar(&self, embedding: &[f32], threshold: f32) -> Result<Option<SimilarMatch>> {
        if embedding.is_empty() {
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
}
