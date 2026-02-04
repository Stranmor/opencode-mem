//! Summary-related PostgreSQL queries.

use crate::event_types::Summary;
use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

type SummaryRow = (
    i64,
    DateTime<Utc>,
    DateTime<Utc>,
    Option<String>,
    Option<String>,
    String,
    i32,
    Option<serde_json::Value>,
);

pub async fn get_unaggregated_5min_summaries(pool: &PgPool, limit: i64) -> Result<Vec<Summary>> {
    let rows = sqlx::query_as::<_, SummaryRow>(
        r#"
        SELECT id, ts_start, ts_end, session_id, project, content, event_count, entities
        FROM summaries_5min
        WHERE summary_hour_id IS NULL
        ORDER BY ts_start ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(row_to_summary).collect())
}

pub async fn get_unaggregated_hour_summaries(pool: &PgPool, limit: i64) -> Result<Vec<Summary>> {
    let rows = sqlx::query_as::<_, SummaryRow>(
        r#"
        SELECT id, ts_start, ts_end, session_id, project, content, event_count, entities
        FROM summaries_hour
        WHERE summary_day_id IS NULL
        ORDER BY ts_start ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(row_to_summary).collect())
}

pub async fn get_summary_5min(pool: &PgPool, id: i64) -> Result<Option<Summary>> {
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

pub async fn get_summary_hour(pool: &PgPool, id: i64) -> Result<Option<Summary>> {
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

pub async fn get_summary_day(pool: &PgPool, id: i64) -> Result<Option<Summary>> {
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
) -> Result<Vec<Summary>> {
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
) -> Result<Vec<Summary>> {
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
) -> Result<Vec<Summary>> {
    const ALLOWED_TYPES: &[&str] = &["files", "functions", "libraries", "errors", "decisions"];
    if !ALLOWED_TYPES.contains(&entity_type) {
        anyhow::bail!(
            "Invalid entity_type '{}'. Allowed: {:?}",
            entity_type,
            ALLOWED_TYPES
        );
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

fn row_to_summary(row: SummaryRow) -> Summary {
    let (id, ts_start, ts_end, session_id, project, content, event_count, entities) = row;
    Summary {
        id,
        ts_start,
        ts_end,
        session_id,
        project,
        content,
        event_count,
        entities: entities.and_then(|e| serde_json::from_value(e).ok()),
    }
}
