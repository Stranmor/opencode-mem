//! Text-based search functions (FTS5)

use anyhow::Result;
use opencode_mem_core::{NoiseLevel, SearchResult};
use rusqlite::params;
use std::collections::HashSet;
use std::str::FromStr as _;

use crate::storage::{
    build_fts_query, coerce_to_sql, get_conn, log_row_error, map_search_result,
    map_search_result_default_score, parse_json, Storage,
};

impl Storage {
    /// Performs FTS5 full-text search.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT o.id, o.title, o.subtitle, o.observation_type, o.noise_level, bm25(observations_fts) as score
               FROM observations_fts f
               JOIN observations o ON o.rowid = f.rowid
               WHERE observations_fts MATCH ?1
               ORDER BY score
               LIMIT ?2",
        )?;
        let results = stmt
            .query_map(params![query, limit], map_search_result)?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    /// Performs hybrid search combining FTS5 and keyword matching.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn hybrid_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let keywords: HashSet<String> = query.split_whitespace().map(str::to_lowercase).collect();

        let fts_query = build_fts_query(query);

        if fts_query.is_empty() {
            return self.get_recent(limit);
        }

        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT o.id, o.title, o.subtitle, o.observation_type, o.noise_level, o.keywords,
                      bm25(observations_fts) as fts_score
               FROM observations_fts f
               JOIN observations o ON o.rowid = f.rowid
               WHERE observations_fts MATCH ?1
               LIMIT ?2",
        )?;

        let raw_results: Vec<(SearchResult, f64, HashSet<String>)> = stmt
            .query_map(params![fts_query, limit * 2], |row| {
                let noise_str: Option<String> = row.get(4)?;
                let noise_level =
                    noise_str.and_then(|s| NoiseLevel::from_str(&s).ok()).unwrap_or_default();
                let obs_keywords: String = row.get(5)?;
                let fts_score: f64 = row.get(6)?;
                Ok((
                    SearchResult::new(
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        parse_json(&row.get::<_, String>(3)?)?,
                        noise_level,
                        0.0,
                    ),
                    fts_score,
                    obs_keywords,
                ))
            })?
            .filter_map(log_row_error)
            .filter_map(|(result, fts_score, obs_keywords)| {
                let obs_kw: HashSet<String> = match parse_json::<Vec<String>>(&obs_keywords) {
                    Ok(v) => v.into_iter().map(|s| s.to_lowercase()).collect(),
                    Err(e) => {
                        tracing::warn!("Failed to parse keywords JSON: {}", e);
                        return None;
                    },
                };
                Some((result, fts_score, obs_kw))
            })
            .collect();

        // Find max FTS score for normalization (BM25 is unbounded, keyword score is 0-1)
        let max_fts_score: f64 =
            raw_results.iter().map(|(_, fts, _)| fts.abs()).fold(0.0, f64::max);

        // Second pass: normalize FTS scores and combine with keyword scores
        let mut results: Vec<(SearchResult, f64)> = raw_results
            .into_iter()
            .map(|(mut result, fts_score, obs_kw)| {
                let fts_normalized =
                    if max_fts_score > 0.0 { fts_score.abs() / max_fts_score } else { 0.0 };
                let keyword_overlap = keywords.intersection(&obs_kw).count() as f64;
                let keyword_score =
                    if keywords.is_empty() { 0.0 } else { keyword_overlap / keywords.len() as f64 };
                result.score = (fts_normalized * 0.7) + (keyword_score * 0.3);
                let score = result.score;
                (result, score)
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results.into_iter().take(limit).map(|(r, _)| r).collect())
    }

    /// Searches with optional filters for project and observation type.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn search_with_filters(
        &self,
        query: Option<&str>,
        project: Option<&str>,
        obs_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let conn = get_conn(&self.pool)?;

        let mut conditions = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(p) = project {
            conditions.push("o.project = ?".to_owned());
            params_vec.push(Box::new(p.to_owned()));
        }
        if let Some(t) = obs_type {
            conditions.push("o.observation_type = ?".to_owned());
            params_vec.push(Box::new(format!("\"{t}\"")));
        }
        if let Some(f) = from {
            conditions.push("o.created_at >= ?".to_owned());
            params_vec.push(Box::new(f.to_owned()));
        }
        if let Some(t) = to {
            conditions.push("o.created_at <= ?".to_owned());
            params_vec.push(Box::new(t.to_owned()));
        }

        if let Some(q) = query {
            let fts_query = build_fts_query(q);

            if !fts_query.is_empty() {
                let where_clause = if conditions.is_empty() {
                    String::new()
                } else {
                    format!("AND {}", conditions.join(" AND "))
                };

                let sql = format!(
                    "SELECT o.id, o.title, o.subtitle, o.observation_type, o.noise_level, bm25(observations_fts) as score
                       FROM observations_fts f
                       JOIN observations o ON o.rowid = f.rowid
                       WHERE observations_fts MATCH ? {where_clause}
                       ORDER BY score
                       LIMIT ?"
                );

                let mut stmt = conn.prepare(&sql)?;
                let mut all_params: Vec<&dyn rusqlite::ToSql> = vec![coerce_to_sql(&fts_query)];
                for p in &params_vec {
                    all_params.push(p.as_ref());
                }
                all_params.push(&limit);

                let results = stmt
                    .query_map(all_params.as_slice(), map_search_result)?
                    .filter_map(log_row_error)
                    .collect();
                return Ok(results);
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT id, title, subtitle, observation_type, noise_level
               FROM observations o {where_clause} ORDER BY created_at DESC LIMIT ?"
        );

        let mut stmt = conn.prepare(&sql)?;
        let mut all_params: Vec<&dyn rusqlite::ToSql> = Vec::new();
        for p in &params_vec {
            all_params.push(p.as_ref());
        }
        all_params.push(&limit);

        let results = stmt
            .query_map(all_params.as_slice(), map_search_result_default_score)?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }
}
