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

