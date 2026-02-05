//! Timeline query functions

use anyhow::Result;
use opencode_mem_core::SearchResult;
use rusqlite::params;

use crate::storage::{get_conn, log_row_error, map_search_result_default_score, Storage};

impl Storage {
    /// Returns observations within a time range.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let conn = get_conn(&self.pool)?;

        let results: Vec<SearchResult> = match (from, to) {
            (Some(f), Some(t)) => {
                let mut stmt = conn.prepare(
                    "SELECT id, title, subtitle, observation_type FROM observations WHERE created_at >= ?1 AND created_at <= ?2 ORDER BY created_at DESC LIMIT ?3"
                )?;
                let res: Vec<SearchResult> = stmt
                    .query_map(params![f, t, limit], map_search_result_default_score)?
                    .filter_map(log_row_error)
                    .collect();
                res
            },
            (Some(f), None) => {
                let mut stmt = conn.prepare(
                    "SELECT id, title, subtitle, observation_type FROM observations WHERE created_at >= ?1 ORDER BY created_at DESC LIMIT ?2"
                )?;
                let res: Vec<SearchResult> = stmt
                    .query_map(params![f, limit], map_search_result_default_score)?
                    .filter_map(log_row_error)
                    .collect();
                res
            },
            (None, Some(t)) => {
                let mut stmt = conn.prepare(
                    "SELECT id, title, subtitle, observation_type FROM observations WHERE created_at <= ?1 ORDER BY created_at DESC LIMIT ?2"
                )?;
                let res: Vec<SearchResult> = stmt
                    .query_map(params![t, limit], map_search_result_default_score)?
                    .filter_map(log_row_error)
                    .collect();
                res
            },
            (None, None) => {
                let mut stmt = conn.prepare(
                    "SELECT id, title, subtitle, observation_type FROM observations ORDER BY created_at DESC LIMIT ?1"
                )?;
                let res: Vec<SearchResult> = stmt
                    .query_map(params![limit], map_search_result_default_score)?
                    .filter_map(log_row_error)
                    .collect();
                res
            },
        };

        Ok(results)
    }
}
