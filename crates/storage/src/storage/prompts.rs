use anyhow::Result;
use chrono::Utc;
use opencode_mem_core::{PromptNumber, UserPrompt};
use rusqlite::params;

use super::{escape_like_pattern, get_conn, log_row_error, Storage};
use crate::pending_queue::PaginatedResult;

impl Storage {
    /// Save user prompt.
    ///
    /// # Errors
    /// Returns error if database insert fails.
    pub fn save_user_prompt(&self, prompt: &UserPrompt) -> Result<()> {
        let conn = get_conn(&self.pool)?;
        conn.execute(
            "INSERT OR REPLACE INTO user_prompts 
               (id, content_session_id, prompt_number, prompt_text, project, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                prompt.id,
                prompt.content_session_id,
                prompt.prompt_number.0,
                prompt.prompt_text,
                prompt.project,
                prompt.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Get prompts with pagination.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>> {
        let conn = get_conn(&self.pool)?;

        let total: i64 = if let Some(p) = project {
            conn.query_row(
                "SELECT COUNT(*) FROM user_prompts WHERE project = ?1",
                params![p],
                |row| row.get(0),
            )?
        } else {
            conn.query_row("SELECT COUNT(*) FROM user_prompts", [], |row| row.get(0))?
        };

        let sql = if project.is_some() {
            "SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
               FROM user_prompts WHERE project = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"
        } else {
            "SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
               FROM user_prompts ORDER BY created_at DESC LIMIT ?1 OFFSET ?2"
        };

        let mut stmt = conn.prepare(sql)?;
        let items: Vec<UserPrompt> = if let Some(p) = project {
            stmt.query_map(params![p, limit, offset], Self::row_to_prompt)?
                .filter_map(log_row_error)
                .collect()
        } else {
            stmt.query_map(params![limit, offset], Self::row_to_prompt)?
                .filter_map(log_row_error)
                .collect()
        };

        Ok(PaginatedResult::new(items, total, offset, limit))
    }

    /// Get prompt by ID.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
               FROM user_prompts WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_prompt(row)?))
        } else {
            Ok(None)
        }
    }

    /// Search prompts by text.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn search_prompts(&self, query: &str, limit: usize) -> Result<Vec<UserPrompt>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let conn = get_conn(&self.pool)?;
        let escaped_query = escape_like_pattern(query);
        let pattern = format!("%{escaped_query}%");
        let mut stmt = conn.prepare(
            r"SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
               FROM user_prompts
               WHERE prompt_text LIKE ?1 ESCAPE '\'
               ORDER BY created_at DESC
               LIMIT ?2",
        )?;
        let results = stmt
            .query_map(params![pattern, limit], Self::row_to_prompt)?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub(crate) fn row_to_prompt(row: &rusqlite::Row<'_>) -> rusqlite::Result<UserPrompt> {
        let created_at = chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
            .with_timezone(&Utc);
        Ok(UserPrompt::new(
            row.get(0)?,
            row.get(1)?,
            PromptNumber(row.get(2)?),
            row.get(3)?,
            row.get(4)?,
            created_at,
        ))
    }
}
