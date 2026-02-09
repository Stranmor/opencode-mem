use anyhow::Result;
use chrono::Utc;
use opencode_mem_core::{Session, SessionStatus};
use rusqlite::params;

use super::{get_conn, Storage};

impl Storage {
    /// Save session.
    ///
    /// # Errors
    /// Returns error if database insert fails.
    pub fn save_session(&self, session: &Session) -> Result<()> {
        let conn = get_conn(&self.pool)?;
        conn.execute(
            "INSERT OR REPLACE INTO sessions 
               (id, content_session_id, memory_session_id, project, user_prompt, started_at, ended_at, status, prompt_counter)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                session.id,
                session.content_session_id,
                session.memory_session_id,
                session.project,
                session.user_prompt,
                session.started_at.with_timezone(&Utc).to_rfc3339(),
                session.ended_at.map(|d| d.with_timezone(&Utc).to_rfc3339()),
                session.status.as_str(),
                session.prompt_counter,
            ],
        )?;
        Ok(())
    }

    /// Get session by ID.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT id, content_session_id, memory_session_id, project, user_prompt, started_at, ended_at, status, prompt_counter
               FROM sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_session(row)?))
        } else {
            Ok(None)
        }
    }

    /// Get session by content session ID.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_session_by_content_id(&self, content_session_id: &str) -> Result<Option<Session>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT id, content_session_id, memory_session_id, project, user_prompt, started_at, ended_at, status, prompt_counter
               FROM sessions WHERE content_session_id = ?1",
        )?;
        let mut rows = stmt.query(params![content_session_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_session(row)?))
        } else {
            Ok(None)
        }
    }

    /// Update session status.
    ///
    /// # Errors
    /// Returns error if database update fails.
    pub fn update_session_status(&self, id: &str, status: SessionStatus) -> Result<()> {
        let conn = get_conn(&self.pool)?;
        conn.execute(
            "UPDATE sessions SET status = ?1, ended_at = ?2 WHERE id = ?3",
            params![
                status.as_str(),
                (status != SessionStatus::Active).then(|| Utc::now().to_rfc3339()),
                id
            ],
        )?;
        Ok(())
    }

    /// Delete session.
    ///
    /// # Errors
    /// Returns error if database delete fails.
    pub fn delete_session(&self, session_id: &str) -> Result<bool> {
        let conn = get_conn(&self.pool)?;
        let affected = conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])?;
        Ok(affected > 0)
    }

    /// Close stale sessions that have been active for longer than the given duration.
    ///
    /// # Errors
    /// Returns error if database update fails.
    pub fn close_stale_sessions(&self, max_age_hours: i64) -> Result<usize> {
        let conn = get_conn(&self.pool)?;
        let now = Utc::now();
        let threshold = now - chrono::Duration::hours(max_age_hours);
        let threshold_str = threshold.to_rfc3339();
        let ended_at_str = now.to_rfc3339();
        let active_status = SessionStatus::Active.as_str();
        let completed_status = SessionStatus::Completed.as_str();
        let legacy_active = format!("\"{}\"", active_status);

        let affected = conn.execute(
            "UPDATE sessions SET status = ?1, ended_at = ?2 WHERE (status = ?3 OR status = ?4) AND started_at < ?5",
            rusqlite::params![completed_status, ended_at_str, active_status, legacy_active, threshold_str],
        )?;
        Ok(affected)
    }

    fn row_to_session(row: &rusqlite::Row<'_>) -> Result<Session> {
        let started_at_str: String = row.get(5)?;
        let started_at = chrono::DateTime::parse_from_rfc3339(&started_at_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse started_at: {e}"))?
            .with_timezone(&Utc);

        let ended_at_str: Option<String> = row.get(6)?;
        let ended_at = ended_at_str
            .map(|s| chrono::DateTime::parse_from_rfc3339(&s))
            .transpose()
            .map_err(|e| anyhow::anyhow!("Failed to parse ended_at: {e}"))?
            .map(|d| d.with_timezone(&Utc));

        let status_str: String = row.get(7)?;
        let status: SessionStatus = status_str
            .parse()
            .or_else(|_| {
                // Legacy: status was stored as JSON string (e.g., '"active"')
                serde_json::from_str::<SessionStatus>(&status_str)
            })
            .map_err(|e| anyhow::anyhow!("Invalid session status '{status_str}': {e}"))?;

        Ok(Session::new(
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            started_at,
            ended_at,
            status,
            row.get(8)?,
        ))
    }
}
