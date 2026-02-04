use anyhow::Result;
use rusqlite::params;

use super::{get_conn, log_row_error, Storage};
use crate::pending_queue::{PaginatedResult, StorageStats};
use opencode_mem_core::Observation;

impl Storage {
    /// Get storage statistics.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_stats(&self) -> Result<StorageStats> {
        let conn = get_conn(&self.pool)?;
        let observation_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM observations", [], |row| row.get(0))?;
        let session_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
        let summary_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM session_summaries", [], |row| row.get(0))?;
        let prompt_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM user_prompts", [], |row| row.get(0))?;
        let project_count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT project) FROM observations WHERE project IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(StorageStats {
            observation_count: observation_count as u64,
            session_count: session_count as u64,
            summary_count: summary_count as u64,
            prompt_count: prompt_count as u64,
            project_count: project_count as u64,
        })
    }

    /// Get all distinct projects.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_all_projects(&self) -> Result<Vec<String>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT project FROM observations WHERE project IS NOT NULL ORDER BY project",
        )?;
        let results = stmt.query_map([], |row| row.get(0))?.filter_map(log_row_error).collect();
        Ok(results)
    }

    /// Get observations with pagination.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_observations_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<Observation>> {
        let conn = get_conn(&self.pool)?;

        let total: i64 = if let Some(p) = project {
            conn.query_row(
                "SELECT COUNT(*) FROM observations WHERE project = ?1",
                params![p],
                |row| row.get(0),
            )?
        } else {
            conn.query_row("SELECT COUNT(*) FROM observations", [], |row| row.get(0))?
        };

        let sql = if project.is_some() {
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, created_at
               FROM observations WHERE project = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"
        } else {
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, created_at
               FROM observations ORDER BY created_at DESC LIMIT ?1 OFFSET ?2"
        };

        let mut stmt = conn.prepare(sql)?;
        let items: Vec<Observation> = if let Some(p) = project {
            stmt.query_map(params![p, limit, offset], Self::row_to_observation)?
                .filter_map(log_row_error)
                .collect()
        } else {
            stmt.query_map(params![limit, offset], Self::row_to_observation)?
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
}
