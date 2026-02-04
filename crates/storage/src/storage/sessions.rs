use anyhow::Result;
use chrono::Utc;
use opencode_mem_core::{Session, SessionStatus};
use rusqlite::params;

use super::{get_conn, Storage};

impl Storage {
    pub fn save_session(&self, session: &Session) -> Result<()> {
        let conn = get_conn(&self.pool)?;
        conn.execute(
            r#"INSERT OR REPLACE INTO sessions 
               (id, content_session_id, memory_session_id, project, user_prompt, started_at, ended_at, status, prompt_counter)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
            params![
                session.id,
                session.content_session_id,
                session.memory_session_id,
                session.project,
                session.user_prompt,
                session.started_at.with_timezone(&Utc).to_rfc3339(),
                session.ended_at.map(|d| d.with_timezone(&Utc).to_rfc3339()),
                serde_json::to_string(&session.status)?,
                session.prompt_counter,
            ],
        )?;
        Ok(())
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            r#"SELECT id, content_session_id, memory_session_id, project, user_prompt, started_at, ended_at, status, prompt_counter
               FROM sessions WHERE id = ?1"#,
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_session(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn get_session_by_content_id(&self, content_session_id: &str) -> Result<Option<Session>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            r#"SELECT id, content_session_id, memory_session_id, project, user_prompt, started_at, ended_at, status, prompt_counter
               FROM sessions WHERE content_session_id = ?1"#,
        )?;
        let mut rows = stmt.query(params![content_session_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_session(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn update_session_status(&self, id: &str, status: SessionStatus) -> Result<()> {
        let conn = get_conn(&self.pool)?;
        conn.execute(
            "UPDATE sessions SET status = ?1, ended_at = ?2 WHERE id = ?3",
            params![
                serde_json::to_string(&status)?,
                if status != SessionStatus::Active {
                    Some(Utc::now().to_rfc3339())
                } else {
                    None
                },
                id
            ],
        )?;
        Ok(())
    }

    pub fn delete_session(&self, session_id: &str) -> Result<bool> {
        let conn = get_conn(&self.pool)?;
        let affected = conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])?;
        Ok(affected > 0)
    }

    fn row_to_session(row: &rusqlite::Row) -> rusqlite::Result<Session> {
        let started_at_str: String = row.get(5)?;
        let started_at = chrono::DateTime::parse_from_rfc3339(&started_at_str)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
            .with_timezone(&Utc);

        let ended_at_str: Option<String> = row.get(6)?;
        let ended_at = ended_at_str
            .map(|s| chrono::DateTime::parse_from_rfc3339(&s))
            .transpose()
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
            .map(|d| d.with_timezone(&Utc));

        let status_str: String = row.get(7)?;
        let status = serde_json::from_str(&status_str)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

        Ok(Session {
            id: row.get(0)?,
            content_session_id: row.get(1)?,
            memory_session_id: row.get(2)?,
            project: row.get(3)?,
            user_prompt: row.get(4)?,
            started_at,
            ended_at,
            status,
            prompt_counter: row.get(8)?,
        })
    }
}
