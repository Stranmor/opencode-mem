use std::collections::{HashMap, HashSet};

use crate::error::StorageError;
use opencode_mem_core::{ObservationId, SearchResult, sort_by_score_descending};

use super::super::super::{
    PgStorage, collect_skipping_corrupt, row_to_search_result, usize_to_i64,
};
use super::super::utils::build_or_tsquery;

/// Hybrid search v2: FTS BM25 (50%) + vector cosine similarity (50%).
pub(crate) async fn hybrid_search_v2(
    storage: &PgStorage,
    query: &str,
    query_vec: &[f32],
    limit: usize,
) -> Result<Vec<SearchResult>, StorageError> {
    hybrid_search_v2_with_filters(storage, query, query_vec, None, None, None, None, limit).await
}

/// Hybrid search v2 with optional filters for project, type, and date range.
#[allow(
    clippy::too_many_arguments,
    reason = "Internal algorithm needs multiple parameters"
)]
pub(crate) async fn hybrid_search_v2_with_filters(
    storage: &PgStorage,
    query: &str,
    query_vec: &[f32],
    project: Option<&str>,
    obs_type: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    limit: usize,
) -> Result<Vec<SearchResult>, StorageError> {
    let fetch_limit = usize_to_i64(limit.saturating_mul(3));

    let mut where_parts: Vec<String> = Vec::new();
    let mut param_idx: usize = 1;
    let mut bind_values: Vec<String> = Vec::new();

    if let Some(p) = project {
        where_parts.push(format!("(project = ${param_idx} OR project IS NULL)"));
        param_idx += 1;
        bind_values.push(p.to_owned());
    }
    if let Some(t) = obs_type {
        where_parts.push(format!("observation_type = ${param_idx}"));
        param_idx += 1;
        bind_values.push(t.to_owned());
    }
    if let Some(f) = from {
        where_parts.push(format!("created_at >= ${param_idx}::timestamptz"));
        param_idx += 1;
        bind_values.push(f.to_owned());
    }
    if let Some(t) = to {
        where_parts.push(format!("created_at <= ${param_idx}::timestamptz"));
        param_idx += 1;
        bind_values.push(t.to_owned());
    }

    let filter_clause = if where_parts.is_empty() {
        String::new()
    } else {
        format!("AND {}", where_parts.join(" AND "))
    };

    let fts_results = match build_or_tsquery(query, 15) {
        Some(tsquery) => {
            let fts_sql = format!(
                "SELECT id, title, subtitle, observation_type, noise_level,
                        ts_rank_cd(search_vec, to_tsquery('simple', ${p}))::float8 as score
                   FROM observations
                   WHERE search_vec @@ to_tsquery('simple', ${p}) {f}
                   ORDER BY score DESC
                   LIMIT ${n}",
                p = param_idx,
                f = filter_clause,
                n = param_idx + 1,
            );
            let mut q = sqlx::query(&fts_sql);
            for val in &bind_values {
                q = q.bind(val);
            }
            q = q.bind(&tsquery);
            q = q.bind(fetch_limit);
            let rows = q.fetch_all(&storage.pool).await?;
            collect_skipping_corrupt(rows.iter().map(row_to_search_result))?
        }
        None => Vec::new(),
    };

    let vector_results = if query_vec.is_empty() {
        Vec::new()
    } else {
        let query_vector = pgvector::Vector::from(query_vec.to_vec());
        let vec_sql = format!(
            "SELECT id, title, subtitle, observation_type, noise_level,
                    (1.0 - (embedding <=> ${p}))::float8 as score
               FROM observations
               WHERE embedding IS NOT NULL {f}
               ORDER BY embedding <=> ${p}
               LIMIT ${n}",
            p = param_idx,
            f = filter_clause,
            n = param_idx + 1,
        );
        let mut q = sqlx::query(&vec_sql);
        for val in &bind_values {
            q = q.bind(val);
        }
        q = q.bind(&query_vector);
        q = q.bind(fetch_limit);
        let rows = q.fetch_all(&storage.pool).await?;
        collect_skipping_corrupt(rows.iter().map(row_to_search_result))?
    };

    Ok(merge_and_rank(fts_results, vector_results, limit))
}

/// Merge FTS and vector results by ID, normalize scores 0-1, combine 50/50.
fn merge_and_rank(
    fts_results: Vec<SearchResult>,
    vector_results: Vec<SearchResult>,
    limit: usize,
) -> Vec<SearchResult> {
    let mut fts_scores: HashMap<ObservationId, (SearchResult, f64)> = HashMap::new();
    let mut vec_scores: HashMap<ObservationId, (SearchResult, f64)> = HashMap::new();

    for r in fts_results {
        let score = r.score;
        fts_scores.insert(r.id.clone(), (r, score));
    }
    for r in vector_results {
        let score = r.score;
        vec_scores.insert(r.id.clone(), (r, score));
    }

    let fts_vals: Vec<f64> = fts_scores.values().map(|(_, s)| *s).collect();
    let (fts_min, fts_max) = fts_vals
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(mn, mx), s| {
            (mn.min(*s), mx.max(*s))
        });
    let fts_range = fts_max - fts_min;

    let vec_vals: Vec<f64> = vec_scores.values().map(|(_, s)| *s).collect();
    let (vec_min, vec_max) = vec_vals
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(mn, mx), s| {
            (mn.min(*s), mx.max(*s))
        });
    let vec_range = vec_max - vec_min;

    let all_ids: HashSet<ObservationId> = fts_scores
        .keys()
        .chain(vec_scores.keys())
        .cloned()
        .collect();

    let combined: Vec<SearchResult> = all_ids
        .into_iter()
        .map(|id| {
            let fts_norm = fts_scores
                .get(&id)
                .map(|(_, s)| {
                    if fts_range > 0.0 {
                        (*s - fts_min) / fts_range
                    } else {
                        1.0
                    }
                })
                .unwrap_or(0.0);
            let vec_norm = vec_scores
                .get(&id)
                .map(|(_, s)| {
                    if vec_range > 0.0 {
                        (*s - vec_min) / vec_range
                    } else {
                        1.0
                    }
                })
                .unwrap_or(0.0);
            let final_score = fts_norm.mul_add(0.5, vec_norm * 0.5);

            let mut result = if let Some((r, _)) = fts_scores.remove(&id) {
                r
            } else if let Some((r, _)) = vec_scores.remove(&id) {
                r
            } else {
                // Unreachable: id came from one of these maps
                return SearchResult::new(
                    id,
                    String::new(),
                    None,
                    opencode_mem_core::ObservationType::Discovery,
                    opencode_mem_core::NoiseLevel::Medium,
                    0.0,
                );
            };
            result.score = final_score;
            result
        })
        .collect();

    let mut combined = combined;
    sort_by_score_descending(&mut combined);
    combined.into_iter().take(limit).collect()
}
