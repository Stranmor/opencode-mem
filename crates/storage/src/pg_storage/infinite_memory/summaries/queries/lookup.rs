use super::super::{SummaryRow, row_to_summary};
use crate::StorageError;
use opencode_mem_core::InfiniteSummary;
use sqlx::PgPool;

pub async fn get_infinite_summary_5min(
    pool: &PgPool,
    id: i64,
) -> Result<Option<InfiniteSummary>, StorageError> {
    let row = sqlx::query_as::<_, SummaryRow>(
        r#"
        SELECT id, ts_start, ts_end, session_id, project, content, event_count, entities
        FROM summaries_5min
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(row_to_summary))
}

pub async fn get_infinite_summary_hour(
    pool: &PgPool,
    id: i64,
) -> Result<Option<InfiniteSummary>, StorageError> {
    let row = sqlx::query_as::<_, SummaryRow>(
        r#"
        SELECT id, ts_start, ts_end, session_id, project, content, event_count, entities
        FROM summaries_hour
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(row_to_summary))
}

pub async fn get_infinite_summary_day(
    pool: &PgPool,
    id: i64,
) -> Result<Option<InfiniteSummary>, StorageError> {
    let row = sqlx::query_as::<_, SummaryRow>(
        r#"
        SELECT id, ts_start, ts_end, session_id, project, content, event_count, entities
        FROM summaries_day
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(row_to_summary))
}

pub async fn get_5min_summaries_by_hour_id(
    pool: &PgPool,
    hour_id: i64,
    limit: i64,
) -> Result<Vec<InfiniteSummary>, StorageError> {
    let rows = sqlx::query_as::<_, SummaryRow>(
        r#"
        SELECT id, ts_start, ts_end, session_id, project, content, event_count, entities
        FROM summaries_5min
        WHERE summary_hour_id = $1
        ORDER BY ts_start ASC
        LIMIT $2
        "#,
    )
    .bind(hour_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(row_to_summary).collect())
}

pub async fn get_hour_summaries_by_day_id(
    pool: &PgPool,
    day_id: i64,
    limit: i64,
) -> Result<Vec<InfiniteSummary>, StorageError> {
    let rows = sqlx::query_as::<_, SummaryRow>(
        r#"
        SELECT id, ts_start, ts_end, session_id, project, content, event_count, entities
        FROM summaries_hour
        WHERE summary_day_id = $1
        ORDER BY ts_start ASC
        LIMIT $2
        "#,
    )
    .bind(day_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(row_to_summary).collect())
}

pub async fn search_by_entity(
    pool: &PgPool,
    entity_type: &str,
    value: &str,
    limit: i64,
) -> Result<Vec<InfiniteSummary>, StorageError> {
    const ALLOWED_TYPES: &[&str] = &["files", "functions", "libraries", "errors", "decisions"];
    if !ALLOWED_TYPES.contains(&entity_type) {
        return Err(StorageError::DataCorruption {
            context: format!(
                "Invalid entity_type '{}'. Allowed: {:?}",
                entity_type, ALLOWED_TYPES
            ),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid entity type",
            )),
        });
    }

    let json_array = serde_json::json!([value]);
    let query = format!(
        r#"
        SELECT id, ts_start, ts_end, session_id, project, content, event_count, entities
        FROM summaries_5min
        WHERE entities->'{entity_type}' @> $1::jsonb
        ORDER BY ts_start DESC
        LIMIT $2
        "#
    );

    let rows = sqlx::query_as::<_, SummaryRow>(&query)
        .bind(&json_array)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    Ok(rows.into_iter().map(row_to_summary).collect())
}
