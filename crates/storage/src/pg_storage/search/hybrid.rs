use std::collections::{HashMap, HashSet};

use crate::error::StorageError;
use opencode_mem_core::SearchResult;
use sqlx::Row;

use super::super::{
    parse_pg_noise_level, parse_pg_observation_type, row_to_search_result_with_score,
    sort_by_score_descending, usize_to_i64, PgStorage,
};
use super::fts::search_with_filters;
use super::utils::build_tsquery;

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

pub(crate) async fn hybrid_search_v2(
    storage: &PgStorage,
    query: &str,
    query_vec: &[f32],
    limit: usize,
) -> Result<Vec<SearchResult>, StorageError> {
    if query_vec.is_empty() {
        return hybrid_search(storage, query, limit).await;
    }

    let mut fts_scores: HashMap<String, f64> = HashMap::new();
    let mut max_fts_score: f64 = 0.0;

    if let Some(tsquery) = build_tsquery(query) {
        let fts_rows = sqlx::query(
            "SELECT id, ts_rank_cd(search_vec, to_tsquery('english', $1))::float8 as fts_score
               FROM observations
               WHERE search_vec @@ to_tsquery('english', $1)
               ORDER BY fts_score DESC
               LIMIT $2",
        )
        .bind(&tsquery)
        .bind(usize_to_i64(limit.saturating_mul(3)))
        .fetch_all(&storage.pool)
        .await?;

        for row in &fts_rows {
            let id: String = row.try_get("id")?;
            let score: f64 = row.try_get("fts_score")?;
            if score > max_fts_score {
                max_fts_score = score;
            }
            fts_scores.insert(id, score);
        }
    }

    let vec_str =
        format!("[{}]", query_vec.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));
    let vec_rows = sqlx::query(
        "SELECT id, 1.0 - (embedding <=> $1::vector) as similarity
           FROM observations
           WHERE embedding IS NOT NULL
           ORDER BY embedding <=> $1::vector
           LIMIT $2",
    )
    .bind(&vec_str)
    .bind(usize_to_i64(limit.saturating_mul(3)))
    .fetch_all(&storage.pool)
    .await?;

    let mut vec_scores: HashMap<String, f64> = HashMap::new();
    for row in &vec_rows {
        let id: String = row.try_get("id")?;
        let sim: f64 = row.try_get("similarity")?;
        vec_scores.insert(id, sim);
    }

    let all_ids: HashSet<String> = fts_scores.keys().chain(vec_scores.keys()).cloned().collect();

    let mut combined: Vec<(String, f64)> = all_ids
        .into_iter()
        .map(|id| {
            let fts_normalized = if max_fts_score > 0.0_f64 {
                fts_scores.get(&id).copied().unwrap_or(0.0_f64) / max_fts_score
            } else {
                0.0_f64
            };
            let vec_sim = vec_scores.get(&id).copied().unwrap_or(0.0_f64);
            let final_score = fts_normalized.mul_add(0.5_f64, vec_sim * 0.5_f64);
            (id, final_score)
        })
        .collect();

    sort_by_score_descending(&mut combined);

    let top: Vec<(String, f64)> = combined.into_iter().take(limit).collect();
    if top.is_empty() {
        return Ok(Vec::new());
    }

    let top_ids: Vec<&str> = top.iter().map(|(id, _)| id.as_str()).collect();
    let score_lookup: HashMap<&str, f64> =
        top.iter().map(|(id, score)| (id.as_str(), *score)).collect();

    let rows = sqlx::query(
        "SELECT id, title, subtitle, observation_type, noise_level
           FROM observations WHERE id = ANY($1)",
    )
    .bind(&top_ids)
    .fetch_all(&storage.pool)
    .await?;

    let mut results: Vec<SearchResult> = rows
        .iter()
        .map(|row| {
            let id: String = row.try_get("id")?;
            let score = score_lookup.get(id.as_str()).copied().unwrap_or(0.0_f64);
            row_to_search_result_with_score(row, score)
        })
        .collect::<Result<_, StorageError>>()?;

    sort_by_score_descending(&mut results);
    Ok(results)
}

#[allow(clippy::too_many_arguments, reason = "delegating method from trait")]
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
    if query_vec.is_empty() {
        return search_with_filters(storage, Some(query), project, obs_type, from, to, limit).await;
    }

    let mut conditions = Vec::new();
    let mut param_idx: usize = 1;
    let mut bind_strings: Vec<String> = Vec::new();

    if let Some(p) = project {
        conditions.push(format!("project = ${param_idx}"));
        param_idx += 1;
        bind_strings.push(p.to_owned());
    }
    if let Some(t) = obs_type {
        conditions.push(format!("observation_type = ${param_idx}"));
        param_idx += 1;
        bind_strings.push(t.to_owned());
    }
    if let Some(f) = from {
        conditions.push(format!("created_at >= ${param_idx}::timestamptz"));
        param_idx += 1;
        bind_strings.push(f.to_owned());
    }
    if let Some(t) = to {
        conditions.push(format!("created_at <= ${param_idx}::timestamptz"));
        param_idx += 1;
        bind_strings.push(t.to_owned());
    }

    let extra_where = if conditions.is_empty() {
        String::new()
    } else {
        format!("AND {}", conditions.join(" AND "))
    };

    let mut fts_scores: HashMap<String, f64> = HashMap::new();
    let mut max_fts_score: f64 = 0.0;

    if let Some(tsquery) = build_tsquery(query) {
        let fts_cond = format!("search_vec @@ to_tsquery('english', ${param_idx})");
        let score_expr = format!(
            "ts_rank_cd(search_vec, to_tsquery('english', ${param_idx}))::float8 as fts_score"
        );
        let limit_param = param_idx + 1;

        let sql = format!(
            "SELECT id, {score_expr}
               FROM observations
               WHERE {fts_cond} {extra_where}
               ORDER BY fts_score DESC
               LIMIT ${limit_param}"
        );

        let mut q = sqlx::query(&sql);
        for val in &bind_strings {
            q = q.bind(val);
        }
        q = q.bind(&tsquery);
        q = q.bind(usize_to_i64(limit.saturating_mul(3)));
        let fts_rows = q.fetch_all(&storage.pool).await?;

        for row in &fts_rows {
            let id: String = row.try_get("id")?;
            let score: f64 = row.try_get("fts_score")?;
            if score > max_fts_score {
                max_fts_score = score;
            }
            fts_scores.insert(id, score);
        }
    }

    let vec_str =
        format!("[{}]", query_vec.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));

    let vec_cond = "embedding IS NOT NULL";
    let extra_where_vec = if conditions.is_empty() {
        String::new()
    } else {
        format!("AND {}", conditions.join(" AND "))
    };
    let limit_param = param_idx + 1;

    let vec_sql = format!(
        "SELECT id, 1.0 - (embedding <=> ${param_idx}::vector) as similarity
           FROM observations
           WHERE {vec_cond} {extra_where_vec}
           ORDER BY embedding <=> ${param_idx}::vector
           LIMIT ${limit_param}"
    );

    let mut q = sqlx::query(&vec_sql);
    for val in &bind_strings {
        q = q.bind(val);
    }
    q = q.bind(&vec_str);
    q = q.bind(usize_to_i64(limit.saturating_mul(3)));
    let vec_rows = q.fetch_all(&storage.pool).await?;

    let mut vec_scores: HashMap<String, f64> = HashMap::new();
    for row in &vec_rows {
        let id: String = row.try_get("id")?;
        let sim: f64 = row.try_get("similarity")?;
        vec_scores.insert(id, sim);
    }

    let all_ids: HashSet<String> = fts_scores.keys().chain(vec_scores.keys()).cloned().collect();

    let mut combined: Vec<(String, f64)> = all_ids
        .into_iter()
        .map(|id| {
            let fts_normalized = if max_fts_score > 0.0_f64 {
                fts_scores.get(&id).copied().unwrap_or(0.0_f64) / max_fts_score
            } else {
                0.0_f64
            };
            let vec_sim = vec_scores.get(&id).copied().unwrap_or(0.0_f64);
            let final_score = fts_normalized.mul_add(0.5_f64, vec_sim * 0.5_f64);
            (id, final_score)
        })
        .collect();

    sort_by_score_descending(&mut combined);

    let top: Vec<(String, f64)> = combined.into_iter().take(limit).collect();
    if top.is_empty() {
        return Ok(Vec::new());
    }

    let top_ids: Vec<&str> = top.iter().map(|(id, _)| id.as_str()).collect();
    let score_lookup: HashMap<&str, f64> =
        top.iter().map(|(id, score)| (id.as_str(), *score)).collect();

    let rows = sqlx::query(
        "SELECT id, title, subtitle, observation_type, noise_level
           FROM observations WHERE id = ANY($1)",
    )
    .bind(&top_ids)
    .fetch_all(&storage.pool)
    .await?;

    let mut results: Vec<SearchResult> = rows
        .iter()
        .map(|row| {
            let id: String = row.try_get("id")?;
            let score = score_lookup.get(id.as_str()).copied().unwrap_or(0.0_f64);
            row_to_search_result_with_score(row, score)
        })
        .collect::<Result<_, StorageError>>()?;

    sort_by_score_descending(&mut results);
    Ok(results)
}
