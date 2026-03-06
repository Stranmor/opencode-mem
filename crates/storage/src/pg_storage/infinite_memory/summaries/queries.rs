use super::{SummaryRow, row_to_summary};
use crate::StorageError;
use opencode_mem_core::InfiniteSummary;
use sqlx::PgPool;

pub async fn get_unaggregated_5min_summaries(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<InfiniteSummary>, StorageError> {
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

pub async fn get_sessions_with_unaggregated_5min(
    pool: &PgPool,
) -> Result<Vec<Option<String>>, StorageError> {
    let rows: Vec<(Option<String>,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT session_id
        FROM summaries_5min
        WHERE summary_hour_id IS NULL
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(sid,)| sid).collect())
}

pub async fn release_summaries_5min(
    pool: &PgPool,
    ids: &[i64],
    increment_retry: bool,
) -> Result<(), StorageError> {
    if ids.is_empty() {
        return Ok(());
    }
    let query = if increment_retry {
        "UPDATE summaries_5min SET processing_started_at = NOW(), processing_instance_id = NULL, retry_count = retry_count + 1 WHERE id = ANY($1)"
    } else {
        "UPDATE summaries_5min SET processing_started_at = NOW(), processing_instance_id = NULL WHERE id = ANY($1)"
    };
    sqlx::query(query).bind(ids).execute(pool).await?;
    Ok(())
}

pub async fn release_summaries_hour(
    pool: &PgPool,
    ids: &[i64],
    increment_retry: bool,
) -> Result<(), StorageError> {
    if ids.is_empty() {
        return Ok(());
    }
    let query = if increment_retry {
        "UPDATE summaries_hour SET processing_started_at = NOW(), processing_instance_id = NULL, retry_count = retry_count + 1 WHERE id = ANY($1)"
    } else {
        "UPDATE summaries_hour SET processing_started_at = NOW(), processing_instance_id = NULL WHERE id = ANY($1)"
    };
    sqlx::query(query).bind(ids).execute(pool).await?;
    Ok(())
}

pub async fn get_unaggregated_5min_for_session(
    pool: &PgPool,
    session_id: Option<&str>,
) -> Result<Vec<InfiniteSummary>, StorageError> {
    let instance_id = uuid::Uuid::new_v4().to_string();
    let visibility_timeout = chrono::Duration::minutes(5);
    let stale_threshold = chrono::Utc::now() - visibility_timeout;

    let rows = if let Some(sid) = session_id {
        sqlx::query_as::<_, SummaryRow>(
            r#"
            UPDATE summaries_5min
            SET processing_started_at = NOW(), processing_instance_id = $3
            WHERE id IN (
                SELECT id
                FROM summaries_5min
                WHERE summary_hour_id IS NULL AND session_id = $1
                  AND retry_count < 3
                  AND (processing_started_at IS NULL OR processing_started_at < $2)
                ORDER BY ts_start ASC
                LIMIT 500
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, ts_start, ts_end, session_id, project, content, event_count, entities
            "#,
        )
        .bind(sid)
        .bind(stale_threshold)
        .bind(&instance_id)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, SummaryRow>(
            r#"
            UPDATE summaries_5min
            SET processing_started_at = NOW(), processing_instance_id = $2
            WHERE id IN (
                SELECT id
                FROM summaries_5min
                WHERE summary_hour_id IS NULL AND session_id IS NULL
                  AND retry_count < 3
                  AND (processing_started_at IS NULL OR processing_started_at < $1)
                ORDER BY ts_start ASC
                LIMIT 500
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, ts_start, ts_end, session_id, project, content, event_count, entities
            "#,
        )
        .bind(stale_threshold)
        .bind(&instance_id)
        .fetch_all(pool)
        .await?
    };

    Ok(rows.into_iter().map(row_to_summary).collect())
}

pub async fn get_unaggregated_hour_summaries(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<InfiniteSummary>, StorageError> {
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

pub async fn get_sessions_with_unaggregated_hour(
    pool: &PgPool,
) -> Result<Vec<Option<String>>, StorageError> {
    let rows: Vec<(Option<String>,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT session_id
        FROM summaries_hour
        WHERE summary_day_id IS NULL
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(sid,)| sid).collect())
}

pub async fn get_unaggregated_hour_for_session(
    pool: &PgPool,
    session_id: Option<&str>,
) -> Result<Vec<InfiniteSummary>, StorageError> {
    let instance_id = uuid::Uuid::new_v4().to_string();
    let visibility_timeout = chrono::Duration::minutes(5);
    let stale_threshold = chrono::Utc::now() - visibility_timeout;

    let rows = if let Some(sid) = session_id {
        sqlx::query_as::<_, SummaryRow>(
            r#"
            UPDATE summaries_hour
            SET processing_started_at = NOW(), processing_instance_id = $3
            WHERE id IN (
                SELECT id
                FROM summaries_hour
                WHERE summary_day_id IS NULL AND session_id = $1
                  AND retry_count < 3
                  AND (processing_started_at IS NULL OR processing_started_at < $2)
                ORDER BY ts_start ASC
                LIMIT 500
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, ts_start, ts_end, session_id, project, content, event_count, entities
            "#,
        )
        .bind(sid)
        .bind(stale_threshold)
        .bind(&instance_id)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, SummaryRow>(
            r#"
            UPDATE summaries_hour
            SET processing_started_at = NOW(), processing_instance_id = $2
            WHERE id IN (
                SELECT id
                FROM summaries_hour
                WHERE summary_day_id IS NULL AND session_id IS NULL
                  AND retry_count < 3
                  AND (processing_started_at IS NULL OR processing_started_at < $1)
                ORDER BY ts_start ASC
                LIMIT 500
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, ts_start, ts_end, session_id, project, content, event_count, entities
            "#,
        )
        .bind(stale_threshold)
        .bind(&instance_id)
        .fetch_all(pool)
        .await?
    };

    Ok(rows.into_iter().map(row_to_summary).collect())
}

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
