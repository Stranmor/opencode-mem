use anyhow::Result;
use opencode_mem_core::Observation;
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
        drop(conn.execute("DELETE FROM observations_vec WHERE rowid = ?1", params![rowid]));
        conn.execute(
            "INSERT INTO observations_vec(rowid, embedding) VALUES (?1, ?2)",
            params![rowid, embedding.as_bytes()],
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
}
