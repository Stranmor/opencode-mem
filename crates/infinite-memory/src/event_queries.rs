//! Event-related PostgreSQL queries.

use crate::event_types::{EventType, RawEvent, StoredEvent};
use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::str::FromStr;

type StoredEventRow = (
    i64,
    DateTime<Utc>,
    String,
    Option<String>,
    String,
    serde_json::Value,
    Vec<String>,
    Vec<String>,
);

pub async fn store_event(pool: &PgPool, event: RawEvent) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO raw_events (session_id, project, event_type, content, files, tools)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id
        "#,
    )
    .bind(&event.session_id)
    .bind(&event.project)
    .bind(event.event_type.as_str())
    .bind(&event.content)
    .bind(&event.files)
    .bind(&event.tools)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

pub async fn get_recent(pool: &PgPool, limit: i64) -> Result<Vec<StoredEvent>> {
    let rows = sqlx::query_as::<_, StoredEventRow>(
        r#"
        SELECT id, ts, session_id, project, event_type, content, files, tools
        FROM raw_events
        ORDER BY ts DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().filter_map(row_to_stored_event).collect())
}

pub async fn release_events(pool: &PgPool, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "UPDATE raw_events SET processing_started_at = NULL, processing_instance_id = NULL, retry_count = retry_count + 1 WHERE id = ANY($1)",
    )
    .bind(ids)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_unsummarized_events(pool: &PgPool, limit: i64) -> Result<Vec<StoredEvent>> {
    let instance_id = uuid::Uuid::new_v4().to_string();
    let visibility_timeout = chrono::Duration::minutes(5);
    let stale_threshold = Utc::now() - visibility_timeout;

    let rows = sqlx::query_as::<_, StoredEventRow>(
        r#"
        UPDATE raw_events
        SET processing_started_at = NOW(), processing_instance_id = $3
        WHERE id IN (
            SELECT id FROM raw_events
            WHERE summary_5min_id IS NULL
              AND retry_count < 3
              AND (processing_started_at IS NULL OR processing_started_at < $2)
            ORDER BY ts ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING id, ts, session_id, project, event_type, content, files, tools
        "#,
    )
    .bind(limit)
    .bind(stale_threshold)
    .bind(&instance_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().filter_map(row_to_stored_event).collect())
}

pub async fn get_events_by_summary_id(
    pool: &PgPool,
    summary_5min_id: i64,
    limit: i64,
) -> Result<Vec<StoredEvent>> {
    let rows = sqlx::query_as::<_, StoredEventRow>(
        r#"
        SELECT id, ts, session_id, project, event_type, content, files, tools
        FROM raw_events
        WHERE summary_5min_id = $1
        ORDER BY ts ASC
        LIMIT $2
        "#,
    )
    .bind(summary_5min_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().filter_map(row_to_stored_event).collect())
}

pub async fn get_events_by_time_range(
    pool: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    session_id: Option<&str>,
    limit: i64,
) -> Result<Vec<StoredEvent>> {
    let rows = if let Some(sid) = session_id {
        sqlx::query_as::<_, StoredEventRow>(
            r#"
            SELECT id, ts, session_id, project, event_type, content, files, tools
            FROM raw_events
            WHERE ts >= $1 AND ts <= $2 AND session_id = $3
            ORDER BY ts ASC
            LIMIT $4
            "#,
        )
        .bind(start)
        .bind(end)
        .bind(sid)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, StoredEventRow>(
            r#"
            SELECT id, ts, session_id, project, event_type, content, files, tools
            FROM raw_events
            WHERE ts >= $1 AND ts <= $2
            ORDER BY ts ASC
            LIMIT $3
            "#,
        )
        .bind(start)
        .bind(end)
        .bind(limit)
        .fetch_all(pool)
        .await?
    };

    Ok(rows.into_iter().filter_map(row_to_stored_event).collect())
}

pub async fn search(pool: &PgPool, query: &str, limit: i64) -> Result<Vec<StoredEvent>> {
    let rows = sqlx::query_as::<_, StoredEventRow>(
        r#"
        SELECT id, ts, session_id, project, event_type, content, files, tools
        FROM raw_events
        WHERE content::text ILIKE '%' || $1 || '%'
        ORDER BY ts DESC
        LIMIT $2
        "#,
    )
    .bind(query)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().filter_map(row_to_stored_event).collect())
}

pub async fn stats(pool: &PgPool) -> Result<serde_json::Value> {
    let event_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM raw_events").fetch_one(pool).await?;

    let summary_5min_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM summaries_5min").fetch_one(pool).await?;

    let summary_hour_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM summaries_hour").fetch_one(pool).await?;

    let summary_day_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM summaries_day").fetch_one(pool).await?;

    Ok(serde_json::json!({
        "raw_events": event_count.0,
        "summaries_5min": summary_5min_count.0,
        "summaries_hour": summary_hour_count.0,
        "summaries_day": summary_day_count.0
    }))
}

fn row_to_stored_event(row: StoredEventRow) -> Option<StoredEvent> {
    let (id, ts, session_id, project, event_type_str, content, files, tools) = row;
    match EventType::from_str(&event_type_str) {
        Ok(event_type) => {
            Some(StoredEvent { id, ts, session_id, project, event_type, content, files, tools })
        },
        Err(_) => {
            tracing::warn!("Unknown event type in DB row {}: '{}'", id, event_type_str);
            None
        },
    }
}
