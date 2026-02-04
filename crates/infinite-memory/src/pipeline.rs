//! Summarization pipeline: create summaries and run compression.

use crate::compression::{compress_events, compress_summaries};
use crate::event_queries;
use crate::event_types::{StoredEvent, Summary, SummaryEntities};
use crate::summary_queries;
use anyhow::Result;
use opencode_mem_llm::LlmClient;

use sqlx::PgPool;
use std::collections::HashMap;

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
    .bind(events.len() as i32)
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

    let ts_start = summaries
        .first()
        .expect("BUG: empty summaries after check")
        .ts_start;
    let ts_end = summaries
        .last()
        .expect("BUG: empty summaries after check")
        .ts_end;
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

    let ts_start = summaries
        .first()
        .expect("BUG: empty summaries after check")
        .ts_start;
    let ts_end = summaries
        .last()
        .expect("BUG: empty summaries after check")
        .ts_end;
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

pub async fn run_compression_pipeline(pool: &PgPool, llm: &LlmClient) -> Result<u32> {
    let events = event_queries::get_unsummarized_events(pool, 100).await?;
    if events.is_empty() {
        return Ok(0);
    }

    let mut sessions: HashMap<String, Vec<StoredEvent>> = HashMap::new();
    for event in events {
        sessions
            .entry(event.session_id.clone())
            .or_default()
            .push(event);
    }

    let mut total_processed = 0u32;
    for (session_id, session_events) in sessions {
        if session_events.is_empty() {
            continue;
        }
        tracing::info!(
            "Compressing {} events for session {}",
            session_events.len(),
            session_id
        );

        let result: Result<()> = async {
            let (summary, entities) = compress_events(llm, &session_events).await?;
            create_5min_summary(pool, &session_events, &summary, entities.as_ref()).await?;
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

        total_processed += session_events.len() as u32;
    }

    Ok(total_processed)
}

pub async fn run_full_compression(pool: &PgPool, llm: &LlmClient) -> Result<(u32, u32, u32)> {
    let events_processed = run_compression_pipeline(pool, llm).await?;

    let summaries_5min = summary_queries::get_unaggregated_5min_summaries(pool, 100).await?;
    let mut sessions_5min: HashMap<Option<String>, Vec<Summary>> = HashMap::new();
    for summary in summaries_5min {
        sessions_5min
            .entry(summary.session_id.clone())
            .or_default()
            .push(summary);
    }

    let mut hours_created = 0u32;
    for (_session_id, session_summaries) in sessions_5min {
        if session_summaries.len() >= MIN_5MIN_SUMMARIES_FOR_HOUR {
            let content = compress_summaries(llm, &session_summaries).await?;
            let merged_entities = SummaryEntities::merge(
                &session_summaries
                    .iter()
                    .map(|s| s.entities.clone())
                    .collect::<Vec<_>>(),
            );
            create_hour_summary(pool, &session_summaries, &content, merged_entities.as_ref())
                .await?;
            hours_created += 1;
        }
    }

    let summaries_hour = summary_queries::get_unaggregated_hour_summaries(pool, 100).await?;
    let mut sessions_hour: HashMap<Option<String>, Vec<Summary>> = HashMap::new();
    for summary in summaries_hour {
        sessions_hour
            .entry(summary.session_id.clone())
            .or_default()
            .push(summary);
    }

    let mut days_created = 0u32;
    for (_session_id, session_summaries) in sessions_hour {
        if session_summaries.len() >= MIN_HOUR_SUMMARIES_FOR_DAY {
            let content = compress_summaries(llm, &session_summaries).await?;
            let merged_entities = SummaryEntities::merge(
                &session_summaries
                    .iter()
                    .map(|s| s.entities.clone())
                    .collect::<Vec<_>>(),
            );
            create_day_summary(pool, &session_summaries, &content, merged_entities.as_ref())
                .await?;
            days_created += 1;
        }
    }

    Ok((events_processed, hours_created, days_created))
}
