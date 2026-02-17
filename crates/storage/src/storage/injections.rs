use anyhow::Result;
use rusqlite::params;

use super::{get_conn, Storage};

impl Storage {
    pub fn save_injected_observations(
        &self,
        session_id: &str,
        observation_ids: &[String],
    ) -> Result<()> {
        if observation_ids.is_empty() {
            return Ok(());
        }
        let conn = get_conn(&self.pool)?;
        let tx = conn.unchecked_transaction()?;
        for obs_id in observation_ids {
            tx.execute(
                "INSERT OR IGNORE INTO injected_observations (session_id, observation_id) VALUES (?1, ?2)",
                params![session_id, obs_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_injected_observation_ids(&self, session_id: &str) -> Result<Vec<String>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt =
            conn.prepare("SELECT observation_id FROM injected_observations WHERE session_id = ?1")?;
        let ids =
            stmt.query_map(params![session_id], |row| row.get(0))?.filter_map(|r| r.ok()).collect();
        Ok(ids)
    }

    pub fn cleanup_old_injections(&self, older_than_hours: u32) -> Result<u64> {
        let conn = get_conn(&self.pool)?;
        let rows = conn.execute(
            "DELETE FROM injected_observations WHERE injected_at < datetime('now', ?1)",
            params![format!("-{older_than_hours} hours")],
        )?;
        Ok(u64::try_from(rows).unwrap_or(0))
    }
}
