//! Summarization pipeline: create summaries and run compression.

use crate::compression::{compress_events, compress_summaries};
use crate::event_queries;
use crate::event_types::{StoredEvent, Summary, SummaryEntities};
use crate::summary_queries;
use anyhow::Result;
use opencode_mem_llm::LlmClient;

use sqlx::PgPool;

const MIN_5MIN_SUMMARIES_FOR_HOUR: usize = 6;
const MIN_HOUR_SUMMARIES_FOR_DAY: usize = 12;

pub async fn create_5min_summary(
    pool: &PgPool,
    events: &[StoredEvent],
    summary: &str,
    entities: Option<&SummaryEntities>,
) -> Result<i64> {
    if events.is_empty() {
        return Ok(0);
    }

    let ts_start = events
        .first()
        .expect("BUG: create_5min_summary called with empty events after is_empty check")
        .ts;
    let ts_end = events
        .last()
        .expect("BUG: create_5min_summary called with empty events after is_empty check")
        .ts;
    let session_id = events.first().map(|e| e.session_id.clone());
    let project = events.first().and_then(|e| e.project.clone());
    let entities_json = entities.and_then(|e| serde_json::to_value(e).ok());

    let mut tx = pool.begin().await?;

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
    .bind(i32::try_from(events.len()).unwrap_or(i32::MAX))
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
    summaries: &[Summary],
    content: &str,
    entities: Option<&SummaryEntities>,
) -> Result<i64> {
    if summaries.is_empty() {
        return Ok(0);
    }

    let ts_start = summaries.first().expect("BUG: empty summaries after check").ts_start;
    let ts_end = summaries.last().expect("BUG: empty summaries after check").ts_end;
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
    summaries: &[Summary],
    content: &str,
    entities: Option<&SummaryEntities>,
) -> Result<i64> {
    if summaries.is_empty() {
        return Ok(0);
    }

    let ts_start = summaries.first().expect("BUG: empty summaries after check").ts_start;
    let ts_end = summaries.last().expect("BUG: empty summaries after check").ts_end;
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

const MAX_EVENTS_PER_BATCH: usize = 100;

pub async fn run_compression_pipeline(pool: &PgPool, llm: &LlmClient) -> Result<u32> {
    let events = event_queries::get_unsummarized_events(
        pool,
        i64::try_from(MAX_EVENTS_PER_BATCH).unwrap_or(i64::MAX),
    )
    .await?;
    if events.is_empty() {
        return Ok(0);
    }

    let mut seen_sessions: Vec<String> = Vec::new();
    for event in &events {
        if !seen_sessions.contains(&event.session_id) {
            seen_sessions.push(event.session_id.clone());
        }
    }

    let mut total_processed = 0u32;
    for session_id in seen_sessions {
        let session_events: Vec<&StoredEvent> =
            events.iter().filter(|e| e.session_id == session_id).collect();

        if session_events.is_empty() {
            continue;
        }

        tracing::info!("Compressing {} events for session {}", session_events.len(), session_id);

        let owned_events: Vec<StoredEvent> = session_events.iter().map(|e| (*e).clone()).collect();

        let result: Result<()> = async {
            let (summary, entities) = compress_events(llm, &owned_events).await?;
            create_5min_summary(pool, &owned_events, &summary, entities.as_ref()).await?;
            Ok(())
        }
        .await;

        if let Err(e) = result {
            tracing::error!(
                session_id = %session_id,
                error = %e,
                "Failed to compress session, skipping"
            );
            continue;
        }

        total_processed += u32::try_from(session_events.len()).unwrap_or(u32::MAX);
    }

    Ok(total_processed)
}

pub async fn run_full_compression(pool: &PgPool, llm: &LlmClient) -> Result<(u32, u32, u32)> {
    let events_processed = run_compression_pipeline(pool, llm).await?;

    let summaries_5min = summary_queries::get_unaggregated_5min_summaries(pool, 100).await?;

    let mut seen_sessions_5min: Vec<Option<String>> = Vec::new();
    for summary in &summaries_5min {
        if !seen_sessions_5min.contains(&summary.session_id) {
            seen_sessions_5min.push(summary.session_id.clone());
        }
    }

    let mut hours_created = 0u32;
    for session_id in seen_sessions_5min {
        let session_summaries: Vec<&Summary> =
            summaries_5min.iter().filter(|s| s.session_id == session_id).collect();

        if session_summaries.len() >= MIN_5MIN_SUMMARIES_FOR_HOUR {
            let owned: Vec<Summary> = session_summaries.iter().map(|s| (*s).clone()).collect();
            let content = compress_summaries(llm, &owned).await?;
            let merged_entities = SummaryEntities::merge(
                &owned.iter().map(|s| s.entities.clone()).collect::<Vec<_>>(),
            );
            create_hour_summary(pool, &owned, &content, merged_entities.as_ref()).await?;
            hours_created += 1;
        }
    }

    let summaries_hour = summary_queries::get_unaggregated_hour_summaries(pool, 100).await?;

    let mut seen_sessions_hour: Vec<Option<String>> = Vec::new();
    for summary in &summaries_hour {
        if !seen_sessions_hour.contains(&summary.session_id) {
            seen_sessions_hour.push(summary.session_id.clone());
        }
    }

    let mut days_created = 0u32;
    for session_id in seen_sessions_hour {
        let session_summaries: Vec<&Summary> =
            summaries_hour.iter().filter(|s| s.session_id == session_id).collect();

        if session_summaries.len() >= MIN_HOUR_SUMMARIES_FOR_DAY {
            let owned: Vec<Summary> = session_summaries.iter().map(|s| (*s).clone()).collect();
            let content = compress_summaries(llm, &owned).await?;
            let merged_entities = SummaryEntities::merge(
                &owned.iter().map(|s| s.entities.clone()).collect::<Vec<_>>(),
            );
            create_day_summary(pool, &owned, &content, merged_entities.as_ref()).await?;
            days_created += 1;
        }
    }

    Ok((events_processed, hours_created, days_created))
}
