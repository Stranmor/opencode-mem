use anyhow::Result;
use chrono::Utc;
use opencode_mem_core::{SessionStatus, SessionSummary};
use rusqlite::params;

use super::{build_fts_query, get_conn, log_row_error, parse_json, Storage};
use crate::pending_queue::PaginatedResult;

impl Storage {
    /// Save session summary.
    ///
    /// # Errors
    /// Returns error if database insert fails.
    pub fn save_summary(&self, summary: &SessionSummary) -> Result<()> {
        let conn = get_conn(&self.pool)?;
        conn.execute(
            "INSERT OR REPLACE INTO session_summaries 
               (session_id, project, request, investigated, learned, completed, next_steps, notes, 
                files_read, files_edited, prompt_number, discovery_tokens, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                summary.session_id,
                summary.project,
                summary.request,
                summary.investigated,
                summary.learned,
                summary.completed,
                summary.next_steps,
                summary.notes,
                serde_json::to_string(&summary.files_read)?,
                serde_json::to_string(&summary.files_edited)?,
                summary.prompt_number,
                summary.discovery_tokens,
                summary.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Get session summary by session ID.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_session_summary(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT session_id, project, request, investigated, learned, completed, next_steps, notes, 
                      files_read, files_edited, prompt_number, discovery_tokens, created_at
               FROM session_summaries WHERE session_id = ?1",
        )?;
        let mut rows = stmt.query(params![session_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_summary(row)?))
        } else {
            Ok(None)
        }
    }

    /// Set session status and optionally save summary text.
    ///
    /// # Errors
    /// Returns error if database operation fails.
    pub fn update_session_status_with_summary(
        &self,
        session_id: &str,
        status: SessionStatus,
        summary: Option<&str>,
    ) -> Result<()> {
        let conn = get_conn(&self.pool)?;
        conn.execute(
            "UPDATE sessions SET status = ?1, ended_at = ?2 WHERE id = ?3",
            params![
                serde_json::to_string(&status)?,
                (status != SessionStatus::Active).then(|| Utc::now().to_rfc3339()),
                session_id
            ],
        )?;

        if let Some(s) = summary {
            let project: Option<String> = conn
                .query_row(
                    "SELECT project FROM sessions WHERE id = ?1",
                    params![session_id],
                    |row| row.get(0),
                )
                .ok();

            if let Some(proj) = project {
                let now = Utc::now();
                conn.execute(
                    "INSERT OR REPLACE INTO session_summaries 
                       (session_id, project, request, investigated, learned, completed, next_steps, notes, 
                        files_read, files_edited, prompt_number, discovery_tokens, created_at)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        session_id,
                        proj,
                        Option::<String>::None,
                        Option::<String>::None,
                        Some(s),
                        Option::<String>::None,
                        Option::<String>::None,
                        Option::<String>::None,
                        "[]",
                        "[]",
                        Option::<i32>::None,
                        Option::<i32>::None,
                        now.to_rfc3339(),
                    ],
                )?;
            }
        }
        Ok(())
    }

    /// Get summaries with pagination.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_summaries_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<SessionSummary>> {
        let conn = get_conn(&self.pool)?;

        let total: i64 = if let Some(p) = project {
            conn.query_row(
                "SELECT COUNT(*) FROM session_summaries WHERE project = ?1",
                params![p],
                |row| row.get(0),
            )?
        } else {
            conn.query_row("SELECT COUNT(*) FROM session_summaries", [], |row| row.get(0))?
        };

        let sql = if project.is_some() {
            "SELECT session_id, project, request, investigated, learned, completed, next_steps, notes, 
                      files_read, files_edited, prompt_number, discovery_tokens, created_at
               FROM session_summaries WHERE project = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"
        } else {
            "SELECT session_id, project, request, investigated, learned, completed, next_steps, notes, 
                      files_read, files_edited, prompt_number, discovery_tokens, created_at
               FROM session_summaries ORDER BY created_at DESC LIMIT ?1 OFFSET ?2"
        };

        let mut stmt = conn.prepare(sql)?;
        let items: Vec<SessionSummary> = if let Some(p) = project {
            stmt.query_map(params![p, limit, offset], Self::row_to_summary)?
                .filter_map(log_row_error)
                .collect()
        } else {
            stmt.query_map(params![limit, offset], Self::row_to_summary)?
                .filter_map(log_row_error)
                .collect()
        };

        Ok(PaginatedResult {
            items,
            total: total as u64,
            offset: offset as u64,
            limit: limit as u64,
        })
    }

    /// Search sessions using FTS.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn search_sessions(&self, query: &str, limit: usize) -> Result<Vec<SessionSummary>> {
        let conn = get_conn(&self.pool)?;
        let fts_query = build_fts_query(query);

        if fts_query.is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = conn.prepare(
            "SELECT s.session_id, s.project, s.request, s.investigated, s.learned, s.completed, 
                      s.next_steps, s.notes, s.files_read, s.files_edited, s.prompt_number, 
                      s.discovery_tokens, s.created_at
               FROM summaries_fts f
               JOIN session_summaries s ON s.rowid = f.rowid
               WHERE summaries_fts MATCH ?1
               ORDER BY bm25(summaries_fts)
               LIMIT ?2",
        )?;
        let results = stmt
            .query_map(params![fts_query, limit], Self::row_to_summary)?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub(crate) fn row_to_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionSummary> {
        let created_at = chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
            .with_timezone(&Utc);
        Ok(SessionSummary::new(
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
            parse_json(&row.get::<_, String>(8)?)?,
            parse_json(&row.get::<_, String>(9)?)?,
            row.get(10)?,
            row.get(11)?,
            created_at,
        ))
    }
}
