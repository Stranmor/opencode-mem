use crate::error::StorageError;
use opencode_mem_core::SearchResult;

use super::super::{row_to_search_result, usize_to_i64, PgStorage};

pub(crate) async fn get_timeline(
    storage: &PgStorage,
    from: Option<&str>,
    to: Option<&str>,
    limit: usize,
) -> Result<Vec<SearchResult>, StorageError> {
    let mut conditions = Vec::new();
    let mut param_idx: usize = 1;
    let mut bind_strings: Vec<String> = Vec::new();

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

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let order_direction = if from.is_some() && to.is_none() { "ASC" } else { "DESC" };

    let sql = format!(
        "SELECT id, title, subtitle, observation_type, noise_level
           FROM observations {where_clause}
           ORDER BY created_at {order_direction}
           LIMIT ${param_idx}"
    );

    let mut q = sqlx::query(&sql);
    for val in &bind_strings {
        q = q.bind(val);
    }
    q = q.bind(usize_to_i64(limit));
    let rows = q.fetch_all(&storage.pool).await?;
    let mut results: Vec<SearchResult> =
        rows.iter().map(row_to_search_result).collect::<Result<_, StorageError>>()?;

    if order_direction == "ASC" {
        results.reverse();
    }

    Ok(results)
}
