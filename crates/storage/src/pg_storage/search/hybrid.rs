use std::collections::{HashMap, HashSet};

use crate::error::StorageError;
use opencode_mem_core::SearchResult;
use sqlx::Row;

use super::super::{
    parse_pg_noise_level, parse_pg_observation_type, row_to_search_result,
    sort_by_score_descending, usize_to_i64, PgStorage,
};
use super::utils::{build_tsquery, build_or_tsquery};

pub(crate) async fn hybrid_search(
    storage: &PgStorage,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, StorageError> {
    let keywords: HashSet<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let Some(tsquery) = build_tsquery(query) else {
        return Ok(Vec::new());
    };

    let rows = sqlx::query(
        "SELECT id, title, subtitle, observation_type, noise_level, keywords,
                ts_rank_cd(search_vec, to_tsquery('english', $1))::float8 as fts_score
           FROM observations
           WHERE search_vec @@ to_tsquery('english', $1)
           ORDER BY fts_score DESC
           LIMIT $2",
    )
    .bind(&tsquery)
    .bind(usize_to_i64(limit.saturating_mul(2)))
    .fetch_all(&storage.pool)
    .await?;

    let raw_results: Vec<(SearchResult, f64, HashSet<String>)> = rows
        .iter()
        .map(|row| {
            let obs_type =
                parse_pg_observation_type(&row.try_get::<String, _>("observation_type")?)?;
            let noise_level =
                parse_pg_noise_level(row.try_get::<Option<String>, _>("noise_level")?.as_deref())?;
            let fts_score: f64 = row.try_get("fts_score")?;
            let kw_json: serde_json::Value = row.try_get("keywords")?;
            let obs_kw: HashSet<String> = serde_json::from_value::<Vec<String>>(kw_json)
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.to_lowercase())
                .collect();
            let sr = SearchResult::new(
                row.try_get("id")?,
                row.try_get("title")?,
                row.try_get("subtitle")?,
                obs_type,
                noise_level,
                0.0,
            );
            Ok((sr, fts_score, obs_kw))
        })
        .collect::<Result<_, StorageError>>()?;

    let (min_fts, max_fts) =
        raw_results.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(mn, mx), (_, fts, _)| {
            (mn.min(*fts), mx.max(*fts))
        });
    let fts_range = max_fts - min_fts;

    let mut results: Vec<(SearchResult, f64)> = raw_results
        .into_iter()
        .map(|(mut result, fts_score, obs_kw)| {
            let fts_normalized: f64 =
                if fts_range > 0.0 { (fts_score - min_fts) / fts_range } else { 1.0 };
            #[expect(
                clippy::cast_precision_loss,
                reason = "keyword count will never exceed f64 precision"
            )]
            let keyword_overlap = keywords.intersection(&obs_kw).count() as f64;
            #[expect(
                clippy::cast_precision_loss,
                reason = "keyword count will never exceed f64 precision"
            )]
            let keyword_score =
                if keywords.is_empty() { 0.0 } else { keyword_overlap / keywords.len() as f64 };
            result.score = fts_normalized.mul_add(0.7, keyword_score * 0.3);
            let score = result.score;
            (result, score)
        })
        .collect();

    sort_by_score_descending(&mut results);
    Ok(results.into_iter().take(limit).map(|(r, _)| r).collect())
}

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
#[allow(clippy::too_many_arguments)]
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
                        ts_rank_cd(search_vec, to_tsquery('english', ${p}))::float8 as score
                   FROM observations
                   WHERE search_vec @@ to_tsquery('english', ${p}) {f}
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
            rows.iter().map(row_to_search_result).collect::<Result<Vec<_>, StorageError>>()?
        },
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
        rows.iter().map(row_to_search_result).collect::<Result<Vec<_>, StorageError>>()?
    };

    Ok(merge_and_rank(fts_results, vector_results, limit))
}

/// Merge FTS and vector results by ID, normalize scores 0-1, combine 50/50.
fn merge_and_rank(
    fts_results: Vec<SearchResult>,
    vector_results: Vec<SearchResult>,
    limit: usize,
) -> Vec<SearchResult> {
    let mut fts_scores: HashMap<String, (SearchResult, f64)> = HashMap::new();
    let mut vec_scores: HashMap<String, (SearchResult, f64)> = HashMap::new();

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
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(mn, mx), s| (mn.min(*s), mx.max(*s)));
    let fts_range = fts_max - fts_min;

    let vec_vals: Vec<f64> = vec_scores.values().map(|(_, s)| *s).collect();
    let (vec_min, vec_max) = vec_vals
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(mn, mx), s| (mn.min(*s), mx.max(*s)));
    let vec_range = vec_max - vec_min;

    let all_ids: HashSet<String> = fts_scores.keys().chain(vec_scores.keys()).cloned().collect();

    let mut combined: Vec<(SearchResult, f64)> = all_ids
        .into_iter()
        .map(|id| {
            let fts_norm = fts_scores
                .get(&id)
                .map(|(_, s)| if fts_range > 0.0 { (*s - fts_min) / fts_range } else { 1.0 })
                .unwrap_or(0.0);
            let vec_norm = vec_scores
                .get(&id)
                .map(|(_, s)| if vec_range > 0.0 { (*s - vec_min) / vec_range } else { 1.0 })
                .unwrap_or(0.0);
            let final_score = fts_norm.mul_add(0.5, vec_norm * 0.5);

            let mut result = if let Some((r, _)) = fts_scores.remove(&id) {
                r
            } else if let Some((r, _)) = vec_scores.remove(&id) {
                r
            } else {
                // Unreachable: id came from one of these maps
                return (
                    SearchResult::new(
                        id,
                        String::new(),
                        None,
                        opencode_mem_core::ObservationType::Discovery,
                        opencode_mem_core::NoiseLevel::Medium,
                        0.0,
                    ),
                    0.0,
                );
            };
            result.score = final_score;
            (result, final_score)
        })
        .collect();

    sort_by_score_descending(&mut combined);
    combined.into_iter().take(limit).map(|(r, _)| r).collect()
}
