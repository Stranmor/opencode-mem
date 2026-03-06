use crate::StorageError;
use opencode_mem_core::{InfiniteSummary, StoredInfiniteEvent, SummaryEntities};
use sqlx::PgPool;

pub async fn create_5min_summary(
    pool: &PgPool,
    events: &[StoredInfiniteEvent],
    summary: &str,
    entities: Option<&SummaryEntities>,
) -> Result<i64, StorageError> {
    if events.is_empty() {
        return Ok(0);
    }

    let ts_start = events
        .first()
        .map(|e| e.ts)
        .ok_or_else(|| StorageError::DataCorruption {
            context: "create_5min_summary called with empty events after is_empty check".to_owned(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "empty events",
            )),
        })?;
    let ts_end = events
        .last()
        .map(|e| e.ts)
        .ok_or_else(|| StorageError::DataCorruption {
            context: "create_5min_summary called with empty events after is_empty check".to_owned(),
            source: Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "empty events",
            )),
        })?;
    let session_id = events.first().map(|e| e.session_id.clone());
    let project = events.first().and_then(|e| e.project.clone());
    let entities_json = entities.and_then(|e| serde_json::to_value(e).ok());

    let mut tx = pool.begin().await?;

    let total_events = i32::try_from(events.len()).map_err(|e| StorageError::DataCorruption {
        context: format!("events.len() {} exceeds i32::MAX: {}", events.len(), e),
        source: Box::new(e),
    })?;

    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO summaries_5min (ts_start, ts_end, session_id, project, content, event_count, entities)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
    )
    .bind(ts_start)
    .bind(ts_end)
    .bind(&session_id)
    .bind(&project)
    .bind(summary)
    .bind(total_events)
    .bind(&entities_json)
    .fetch_one(&mut *tx)
    .await?;

    let summary_id = row.0;

    let event_ids: Vec<i64> = events.iter().map(|e| e.id).collect();
    sqlx::query(
        r#"
        UPDATE raw_events SET summary_5min_id = $1 WHERE id = ANY($2)
        "#,
    )
    .bind(summary_id)
    .bind(&event_ids)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(summary_id)
}

pub async fn create_hour_summary(
    pool: &PgPool,
    summaries: &[InfiniteSummary],
    content: &str,
    entities: Option<&SummaryEntities>,
) -> Result<i64, StorageError> {
    if summaries.is_empty() {
        return Ok(0);
    }

    let ts_start =
        summaries
            .first()
            .map(|s| s.ts_start)
            .ok_or_else(|| StorageError::DataCorruption {
                context: "empty summaries after check".to_owned(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "empty",
                )),
            })?;
    let ts_end =
        summaries
            .last()
            .map(|s| s.ts_end)
            .ok_or_else(|| StorageError::DataCorruption {
                context: "empty summaries after check".to_owned(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "empty",
                )),
            })?;
    let session_id = summaries.first().and_then(|s| s.session_id.clone());
    let project = summaries.first().and_then(|s| s.project.clone());
    let total_events: i32 = summaries.iter().map(|s| s.event_count).sum();
    let entities_json = entities.and_then(|e| serde_json::to_value(e).ok());

    let mut tx = pool.begin().await?;

    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO summaries_hour (ts_start, ts_end, session_id, project, content, event_count, entities)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
    )
    .bind(ts_start)
    .bind(ts_end)
    .bind(&session_id)
    .bind(&project)
    .bind(content)
    .bind(total_events)
    .bind(&entities_json)
    .fetch_one(&mut *tx)
    .await?;

    let hour_id = row.0;
    let summary_ids: Vec<i64> = summaries.iter().map(|s| s.id).collect();
    sqlx::query("UPDATE summaries_5min SET summary_hour_id = $1 WHERE id = ANY($2)")
        .bind(hour_id)
        .bind(&summary_ids)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(hour_id)
}

pub async fn create_day_summary(
    pool: &PgPool,
    summaries: &[InfiniteSummary],
    content: &str,
    entities: Option<&SummaryEntities>,
) -> Result<i64, StorageError> {
    if summaries.is_empty() {
        return Ok(0);
    }

    let ts_start =
        summaries
            .first()
            .map(|s| s.ts_start)
            .ok_or_else(|| StorageError::DataCorruption {
                context: "empty summaries after check".to_owned(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "empty",
                )),
            })?;
    let ts_end =
        summaries
            .last()
            .map(|s| s.ts_end)
            .ok_or_else(|| StorageError::DataCorruption {
                context: "empty summaries after check".to_owned(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "empty",
                )),
            })?;
    let session_id = summaries.first().and_then(|s| s.session_id.clone());
    let project = summaries.first().and_then(|s| s.project.clone());
    let total_events: i32 = summaries.iter().map(|s| s.event_count).sum();
    let entities_json = entities.and_then(|e| serde_json::to_value(e).ok());

    let mut tx = pool.begin().await?;

    let row: (i64,) = sqlx::query_as(
        r#"
        INSERT INTO summaries_day (ts_start, ts_end, session_id, project, content, event_count, entities)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
    )
    .bind(ts_start)
    .bind(ts_end)
    .bind(&session_id)
    .bind(&project)
    .bind(content)
    .bind(total_events)
    .bind(&entities_json)
    .fetch_one(&mut *tx)
    .await?;

    let day_id = row.0;
    let summary_ids: Vec<i64> = summaries.iter().map(|s| s.id).collect();
    sqlx::query("UPDATE summaries_hour SET summary_day_id = $1 WHERE id = ANY($2)")
        .bind(day_id)
        .bind(&summary_ids)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(day_id)
}
