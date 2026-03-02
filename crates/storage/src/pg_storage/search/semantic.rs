use crate::error::StorageError;
use opencode_mem_core::SearchResult;

use super::super::{row_to_search_result, usize_to_i64, PgStorage};

pub(crate) async fn semantic_search(
    storage: &PgStorage,
    query_vec: &[f32],
    limit: usize,
) -> Result<Vec<SearchResult>, StorageError> {
    if query_vec.is_empty() {
        return Ok(Vec::new());
    }

    let query_vector = pgvector::Vector::from(query_vec.to_vec());
    let rows = sqlx::query(
        "SELECT id, title, subtitle, observation_type, noise_level,
                1.0 - (embedding <=> $1) as score
           FROM observations
           WHERE embedding IS NOT NULL
           ORDER BY embedding <=> $1
           LIMIT $2",
    )
    .bind(&query_vector)
    .bind(usize_to_i64(limit))
    .fetch_all(&storage.pool)
    .await?;
    rows.iter().map(row_to_search_result).collect::<Result<_, StorageError>>()
}
