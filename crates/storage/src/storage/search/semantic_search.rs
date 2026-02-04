//! Semantic and hybrid vector search functions

use anyhow::Result;
use opencode_mem_core::SearchResult;
use rusqlite::params;
use std::collections::HashMap;

use crate::storage::{
    build_fts_query, get_conn, log_row_error, map_search_result, parse_json, Storage,
};

impl Storage {
    pub fn semantic_search(&self, query_vec: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        use zerocopy::IntoBytes;

        if query_vec.is_empty() {
            return Ok(Vec::new());
        }

        let conn = get_conn(&self.pool)?;

        let vec_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM observations_vec", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        if vec_count == 0 {
            tracing::debug!("No embeddings in observations_vec, falling back to empty results");
            return Ok(Vec::new());
        }

        let query_bytes = query_vec.as_bytes();

        let mut stmt = conn.prepare(
            r#"SELECT o.id, o.title, o.subtitle, o.observation_type,
                      (1.0 - vec_distance_cosine(v.embedding, ?1)) as similarity
               FROM observations_vec v
               JOIN observations o ON o.rowid = v.rowid
               ORDER BY similarity DESC
               LIMIT ?2"#,
        )?;

        let results = stmt
            .query_map(params![query_bytes, limit], map_search_result)?
            .filter_map(log_row_error)
            .collect();

        Ok(results)
    }

    /// Hybrid search: FTS5 BM25 (50%) + vector cosine similarity (50%)
    pub fn hybrid_search_v2(
        &self,
        query: &str,
        query_vec: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        use zerocopy::IntoBytes;

        if query_vec.is_empty() {
            return self.hybrid_search(query, limit);
        }

        let conn = get_conn(&self.pool)?;

        let vec_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM observations_vec", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        if vec_count == 0 {
            tracing::debug!("No embeddings available, using text-only hybrid search");
            drop(conn);
            return self.hybrid_search(query, limit);
        }

        let fts_query = build_fts_query(query);

        let mut fts_scores: HashMap<String, f64> = HashMap::new();
        let mut max_fts_score: f64 = 0.0;

        if !fts_query.is_empty() {
            let mut stmt = conn.prepare(
                r#"SELECT o.id, ABS(bm25(observations_fts)) as fts_score
                   FROM observations_fts f
                   JOIN observations o ON o.rowid = f.rowid
                   WHERE observations_fts MATCH ?1
                   LIMIT ?2"#,
            )?;

            let fts_results: Vec<(String, f64)> = stmt
                .query_map(params![fts_query, limit * 3], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                })?
                .filter_map(log_row_error)
                .collect();

            for (id, score) in fts_results {
                if score > max_fts_score {
                    max_fts_score = score;
                }
                fts_scores.insert(id, score);
            }
        }

        let query_bytes = query_vec.as_bytes();
        let mut vec_scores: HashMap<String, f64> = HashMap::new();

        let mut stmt = conn.prepare(
            r#"SELECT o.id, (1.0 - vec_distance_cosine(v.embedding, ?1)) as similarity
               FROM observations_vec v
               JOIN observations o ON o.rowid = v.rowid
               ORDER BY similarity DESC
               LIMIT ?2"#,
        )?;

        let vec_results: Vec<(String, f64)> = stmt
            .query_map(params![query_bytes, limit * 3], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
            })?
            .filter_map(log_row_error)
            .collect();

        for (id, score) in vec_results {
            vec_scores.insert(id, score);
        }

        let all_ids: std::collections::HashSet<String> = fts_scores
            .keys()
            .chain(vec_scores.keys())
            .cloned()
            .collect();

        let mut combined: Vec<(String, f64)> = all_ids
            .into_iter()
            .map(|id| {
                let fts_normalized = if max_fts_score > 0.0 {
                    fts_scores.get(&id).copied().unwrap_or(0.0) / max_fts_score
                } else {
                    0.0
                };
                let vec_sim = vec_scores.get(&id).copied().unwrap_or(0.0);
                let final_score = (fts_normalized * 0.5) + (vec_sim * 0.5);
                (id, final_score)
            })
            .collect();

        combined.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let top_ids: Vec<String> = combined.into_iter().take(limit).map(|(id, _)| id).collect();

        if top_ids.is_empty() {
            return Ok(Vec::new());
        }

        let score_lookup: HashMap<String, f64> = fts_scores
            .keys()
            .chain(vec_scores.keys())
            .map(|id| {
                let fts_normalized = if max_fts_score > 0.0 {
                    fts_scores.get(id).copied().unwrap_or(0.0) / max_fts_score
                } else {
                    0.0
                };
                let vec_sim = vec_scores.get(id).copied().unwrap_or(0.0);
                (id.clone(), (fts_normalized * 0.5) + (vec_sim * 0.5))
            })
            .collect();

        let placeholders = top_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, title, subtitle, observation_type FROM observations WHERE id IN ({})",
            placeholders
        );

        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            top_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

        let mut results: Vec<SearchResult> = stmt
            .query_map(params.as_slice(), |row| {
                let id: String = row.get(0)?;
                let score = score_lookup.get(&id).copied().unwrap_or(0.0);
                Ok(SearchResult {
                    id,
                    title: row.get(1)?,
                    subtitle: row.get(2)?,
                    observation_type: parse_json(&row.get::<_, String>(3)?)?,
                    score,
                })
            })?
            .filter_map(log_row_error)
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results)
    }
}
