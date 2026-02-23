use crate::error::StorageError;
use opencode_mem_core::SearchResult;

use super::super::{row_to_search_result, usize_to_i64, PgStorage};
use super::utils::build_tsquery;

pub(crate) async fn search(
    storage: &PgStorage,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, StorageError> {
    let Some(tsquery) = build_tsquery(query) else {
        return Ok(Vec::new());
    };
    let rows = sqlx::query(
        "SELECT id, title, subtitle, observation_type, noise_level,
                ts_rank_cd(search_vec, to_tsquery('english', $1))::float8 as score
           FROM observations
           WHERE search_vec @@ to_tsquery('english', $1)
           ORDER BY score DESC
           LIMIT $2",
    )
    .bind(&tsquery)
    .bind(usize_to_i64(limit))
    .fetch_all(&storage.pool)
    .await?;
    rows.iter().map(row_to_search_result).collect::<Result<_, StorageError>>()
}

pub(crate) async fn search_with_filters(
    storage: &PgStorage,
    query: Option<&str>,
    project: Option<&str>,
    obs_type: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    limit: usize,
) -> Result<Vec<SearchResult>, StorageError> {
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

    if let Some(q) = query {
        if let Some(tsquery) = build_tsquery(q) {
            let fts_cond = format!("search_vec @@ to_tsquery('english', ${param_idx})");
            param_idx += 1;
            let score_expr = format!(
                "ts_rank_cd(search_vec, to_tsquery('english', ${}))::float8 as score",
                param_idx - 1
            );
            let extra_where = if conditions.is_empty() {
                String::new()
            } else {
                format!("AND {}", conditions.join(" AND "))
            };
            let sql = format!(
                "SELECT id, title, subtitle, observation_type, noise_level, {score_expr}
                   FROM observations
                   WHERE {fts_cond} {extra_where}
                   ORDER BY score DESC
                   LIMIT ${param_idx}"
            );

            let mut q = sqlx::query(&sql);
            for val in &bind_strings {
                q = q.bind(val);
            }
            q = q.bind(&tsquery);
            q = q.bind(usize_to_i64(limit));
            let rows = q.fetch_all(&storage.pool).await?;
            return rows.iter().map(row_to_search_result).collect::<Result<_, StorageError>>();
        }
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    let sql = format!(
        "SELECT id, title, subtitle, observation_type, noise_level
           FROM observations {where_clause}
           ORDER BY created_at DESC
           LIMIT ${param_idx}"
    );

    let mut q = sqlx::query(&sql);
    for val in &bind_strings {
        q = q.bind(val);
    }
    q = q.bind(usize_to_i64(limit));
    let rows = q.fetch_all(&storage.pool).await?;
    rows.iter().map(row_to_search_result).collect::<Result<_, StorageError>>()
}
