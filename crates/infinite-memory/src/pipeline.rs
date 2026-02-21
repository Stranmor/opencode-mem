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
    let mut total_processed = 0u32;
    loop {
        let events = event_queries::get_unsummarized_events(
            pool,
            i64::try_from(MAX_EVENTS_PER_BATCH).unwrap_or(i64::MAX),
        )
        .await?;

        if events.is_empty() {
            break;
        }

        let mut seen_sessions: Vec<String> = Vec::new();
        for event in &events {
            if !seen_sessions.contains(&event.session_id) {
                seen_sessions.push(event.session_id.clone());
            }
        }

        let mut processed_in_batch = 0;
        for session_id in seen_sessions {
            let session_events: Vec<&StoredEvent> =
                events.iter().filter(|e| e.session_id == session_id).collect();

            if session_events.is_empty() {
                continue;
            }

            let mut current_bucket: Vec<StoredEvent> = Vec::new();
            let mut bucket_start = session_events[0].ts;
            let mut buckets = Vec::new();

            for event in session_events {
                // Group events into strict 5-minute temporal buckets
                if (event.ts - bucket_start).num_seconds() > 300 {
                    buckets.push(current_bucket.clone());
                    current_bucket.clear();
                    bucket_start = event.ts;
                }
                current_bucket.push((*event).clone());
            }
            if !current_bucket.is_empty() {
                buckets.push(current_bucket);
            }

            for owned_events in buckets {
                tracing::info!(
                    "Compressing {} events for session {} (time window)",
                    owned_events.len(),
                    session_id
                );

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
                        "Failed to compress 5min bucket, skipping"
                    );
                    let ids: Vec<i64> = owned_events.iter().map(|e| e.id).collect();
                    let _ = event_queries::release_events(pool, &ids).await;
                } else {
                    processed_in_batch += 1;
                    total_processed += u32::try_from(owned_events.len()).unwrap_or(u32::MAX);
                }
            }
        }

        if processed_in_batch == 0 {
            tracing::warn!(
                "Failed to process any events in batch, breaking to prevent infinite loop"
            );
            break;
        }
    }

    Ok(total_processed)
}

pub async fn run_full_compression(pool: &PgPool, llm: &LlmClient) -> Result<(u32, u32, u32)> {
    let events_processed = run_compression_pipeline(pool, llm).await?;

    // Phase 2: Compress 5min summaries → hour summaries (per-session)
    let sessions_5min = summary_queries::get_sessions_with_unaggregated_5min(pool).await?;

    let mut hours_created = 0u32;
    for session_id in sessions_5min {
        let session_summaries =
            summary_queries::get_unaggregated_5min_for_session(pool, session_id.as_deref()).await?;

        let should_aggregate = if session_summaries.len() >= MIN_5MIN_SUMMARIES_FOR_HOUR {
            true
        } else if let Some(first) = session_summaries.first() {
            (chrono::Utc::now() - first.ts_start).num_hours() >= 1
        } else {
            false
        };

        if should_aggregate {
            let result: Result<()> = async {
                let content = compress_summaries(llm, &session_summaries).await?;
                let merged_entities = SummaryEntities::merge(
                    &session_summaries.iter().map(|s| s.entities.clone()).collect::<Vec<_>>(),
                );
                create_hour_summary(pool, &session_summaries, &content, merged_entities.as_ref())
                    .await?;
                Ok(())
            }
            .await;

            if let Err(e) = result {
                tracing::error!(
                    session_id = %session_id.unwrap_or_default(),
                    error = %e,
                    "Failed to create hour summary, releasing records"
                );
                let ids: Vec<i64> = session_summaries.iter().map(|s| s.id).collect();
                let _ = summary_queries::release_summaries_5min(pool, &ids).await;
            } else {
                hours_created += 1;
            }
        } else if !session_summaries.is_empty() {
            let ids: Vec<i64> = session_summaries.iter().map(|s| s.id).collect();
            summary_queries::release_summaries_5min(pool, &ids).await?;
        }
    }

    // Phase 3: Compress hour summaries → day summaries (per-session)
    let sessions_hour = summary_queries::get_sessions_with_unaggregated_hour(pool).await?;

    let mut days_created = 0u32;
    for session_id in sessions_hour {
        let session_summaries =
            summary_queries::get_unaggregated_hour_for_session(pool, session_id.as_deref()).await?;

        let should_aggregate = if session_summaries.len() >= MIN_HOUR_SUMMARIES_FOR_DAY {
            true
        } else if let Some(first) = session_summaries.first() {
            (chrono::Utc::now() - first.ts_start).num_days() >= 1
        } else {
            false
        };

        if should_aggregate {
            let result: Result<()> = async {
                let content = compress_summaries(llm, &session_summaries).await?;
                let merged_entities = SummaryEntities::merge(
                    &session_summaries.iter().map(|s| s.entities.clone()).collect::<Vec<_>>(),
                );
                create_day_summary(pool, &session_summaries, &content, merged_entities.as_ref())
                    .await?;
                Ok(())
            }
            .await;

            if let Err(e) = result {
                tracing::error!(
                    session_id = %session_id.unwrap_or_default(),
                    error = %e,
                    "Failed to create day summary, releasing records"
                );
                let ids: Vec<i64> = session_summaries.iter().map(|s| s.id).collect();
                let _ = summary_queries::release_summaries_hour(pool, &ids).await;
            } else {
                days_created += 1;
            }
        } else if !session_summaries.is_empty() {
            let ids: Vec<i64> = session_summaries.iter().map(|s| s.id).collect();
            summary_queries::release_summaries_hour(pool, &ids).await?;
        }
    }

    Ok((events_processed, hours_created, days_created))
}
