use super::compression::{compress_events, compress_summaries};
use anyhow::Result;
use chrono::{DateTime, Utc};
use opencode_mem_core::{StoredInfiniteEvent, SummaryEntities};
use opencode_mem_llm::LlmClient;
use opencode_mem_storage::pg_storage::infinite_memory;
use sqlx::PgPool;

const MIN_5MIN_SUMMARIES_FOR_HOUR: usize = 6;
const MIN_HOUR_SUMMARIES_FOR_DAY: usize = 12;
const MAX_EVENTS_PER_BATCH: usize = 100;

/// Check if a timestamp is older than `n` hours from now.
/// Uses `signed_duration_since` to avoid arithmetic overflow on `DateTime` subtraction.
fn is_older_than_hours(ts: &DateTime<Utc>, n: i64) -> bool {
    ts.signed_duration_since(Utc::now()).num_hours() <= n.saturating_neg()
}

/// Check if a timestamp is older than `n` days from now.
fn is_older_than_days(ts: &DateTime<Utc>, n: i64) -> bool {
    ts.signed_duration_since(Utc::now()).num_days() <= n.saturating_neg()
}

pub async fn run_compression_pipeline(pool: &PgPool, llm: &LlmClient) -> Result<u32> {
    let mut total_processed = 0u32;
    let batch_limit = i64::try_from(MAX_EVENTS_PER_BATCH)
        .map_err(|e| anyhow::anyhow!("MAX_EVENTS_PER_BATCH exceeds i64::MAX: {e}"))?;
    loop {
        let events = infinite_memory::get_unsummarized_infinite_events(pool, batch_limit)
            .await
            .map_err(anyhow::Error::from)?;
        if events.is_empty() {
            break;
        }

        let mut seen_sessions: Vec<String> = Vec::new();
        for event in &events {
            if !seen_sessions.contains(&event.session_id) {
                seen_sessions.push(event.session_id.clone());
            }
        }

        for session_id in seen_sessions {
            let mut session_events: Vec<&StoredInfiniteEvent> = events
                .iter()
                .filter(|e| e.session_id == session_id)
                .collect();

            if session_events.is_empty() {
                continue;
            }

            session_events.sort_by_key(|e| e.ts);

            let mut current_bucket: Vec<StoredInfiniteEvent> = Vec::new();
            let Some(first_event) = session_events.first() else {
                continue;
            };
            let mut bucket_start = first_event.ts;
            let mut buckets = Vec::new();

            for event in session_events {
                if event.ts.timestamp() / 300 != bucket_start.timestamp() / 300 {
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
                    infinite_memory::create_5min_summary(
                        pool,
                        &owned_events,
                        &summary,
                        entities.as_ref(),
                    )
                    .await
                    .map_err(anyhow::Error::from)?;
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
                    let _ = infinite_memory::release_infinite_events(pool, &ids).await;
                } else {
                    let event_count = u32::try_from(owned_events.len()).map_err(|e| {
                        anyhow::anyhow!(
                            "owned_events.len() {} exceeds u32::MAX: {}",
                            owned_events.len(),
                            e
                        )
                    })?;
                    total_processed =
                        total_processed.checked_add(event_count).ok_or_else(|| {
                            anyhow::anyhow!("total_processed overflow at {}", total_processed)
                        })?;
                }
            }
        }

        if events.len() < MAX_EVENTS_PER_BATCH {
            break;
        }
    }

    Ok(total_processed)
}

pub async fn run_full_compression(pool: &PgPool, llm: &LlmClient) -> Result<(u32, u32, u32)> {
    let events_processed = run_compression_pipeline(pool, llm).await?;

    let sessions_5min = infinite_memory::get_sessions_with_unaggregated_5min(pool)
        .await
        .map_err(anyhow::Error::from)?;

    let mut hours_created = 0u32;
    for session_id in sessions_5min {
        let mut session_summaries =
            infinite_memory::get_unaggregated_5min_for_session(pool, session_id.as_deref())
                .await
                .map_err(anyhow::Error::from)?;

        if session_summaries.is_empty() {
            continue;
        }

        session_summaries.sort_by_key(|s| s.ts_start);

        let mut buckets = Vec::new();
        let mut current_bucket = Vec::new();
        let Some(first_summary) = session_summaries.first() else {
            continue;
        };
        let mut bucket_start = first_summary.ts_start;

        for s in session_summaries {
            if s.ts_start.timestamp() / 3600 != bucket_start.timestamp() / 3600 {
                buckets.push(current_bucket.clone());
                current_bucket.clear();
                bucket_start = s.ts_start;
            }
            current_bucket.push(s);
        }
        if !current_bucket.is_empty() {
            buckets.push(current_bucket);
        }

        for bucket in buckets {
            let should_aggregate = if bucket.len() >= MIN_5MIN_SUMMARIES_FOR_HOUR {
                true
            } else if let Some(first) = bucket.first() {
                is_older_than_hours(&first.ts_start, 1)
            } else {
                false
            };

            if should_aggregate {
                let result: Result<()> = async {
                    let content = compress_summaries(llm, &bucket).await?;
                    let merged_entities = SummaryEntities::merge(
                        &bucket
                            .iter()
                            .map(|s| s.entities.clone())
                            .collect::<Vec<_>>(),
                    );
                    infinite_memory::create_hour_summary(
                        pool,
                        &bucket,
                        &content,
                        merged_entities.as_ref(),
                    )
                    .await
                    .map_err(anyhow::Error::from)?;
                    Ok(())
                }
                .await;

                if let Err(e) = result {
                    tracing::error!(
                        session_id = %session_id.clone().unwrap_or_default(),
                        error = %e,
                        "Failed to create hour summary, releasing records"
                    );
                    let ids: Vec<i64> = bucket.iter().map(|s| s.id).collect();
                    let _ = infinite_memory::release_summaries_5min(pool, &ids, true).await;
                } else {
                    hours_created = hours_created.saturating_add(1);
                }
            } else if !bucket.is_empty() {
                let ids: Vec<i64> = bucket.iter().map(|s| s.id).collect();
                infinite_memory::release_summaries_5min(pool, &ids, false)
                    .await
                    .map_err(anyhow::Error::from)?;
            }
        }
    }

    let sessions_hour = infinite_memory::get_sessions_with_unaggregated_hour(pool)
        .await
        .map_err(anyhow::Error::from)?;

    let mut days_created = 0u32;
    for session_id in sessions_hour {
        let mut session_summaries =
            infinite_memory::get_unaggregated_hour_for_session(pool, session_id.as_deref())
                .await
                .map_err(anyhow::Error::from)?;

        if session_summaries.is_empty() {
            continue;
        }

        session_summaries.sort_by_key(|s| s.ts_start);

        let mut buckets = Vec::new();
        let mut current_bucket = Vec::new();
        let Some(first_summary) = session_summaries.first() else {
            continue;
        };
        let mut bucket_start = first_summary.ts_start;

        for s in session_summaries {
            if s.ts_start.timestamp() / 86400 != bucket_start.timestamp() / 86400 {
                buckets.push(current_bucket.clone());
                current_bucket.clear();
                bucket_start = s.ts_start;
            }
            current_bucket.push(s);
        }
        if !current_bucket.is_empty() {
            buckets.push(current_bucket);
        }

        for bucket in buckets {
            let should_aggregate = if bucket.len() >= MIN_HOUR_SUMMARIES_FOR_DAY {
                true
            } else if let Some(first) = bucket.first() {
                is_older_than_days(&first.ts_start, 1)
            } else {
                false
            };

            if should_aggregate {
                let result: Result<()> = async {
                    let content = compress_summaries(llm, &bucket).await?;
                    let merged_entities = SummaryEntities::merge(
                        &bucket
                            .iter()
                            .map(|s| s.entities.clone())
                            .collect::<Vec<_>>(),
                    );
                    infinite_memory::create_day_summary(
                        pool,
                        &bucket,
                        &content,
                        merged_entities.as_ref(),
                    )
                    .await
                    .map_err(anyhow::Error::from)?;
                    Ok(())
                }
                .await;

                if let Err(e) = result {
                    tracing::error!(
                        session_id = %session_id.clone().unwrap_or_default(),
                        error = %e,
                        "Failed to create day summary, releasing records"
                    );
                    let ids: Vec<i64> = bucket.iter().map(|s| s.id).collect();
                    let _ = infinite_memory::release_summaries_hour(pool, &ids, true).await;
                } else {
                    days_created = days_created.saturating_add(1);
                }
            } else if !bucket.is_empty() {
                let ids: Vec<i64> = bucket.iter().map(|s| s.id).collect();
                infinite_memory::release_summaries_hour(pool, &ids, false)
                    .await
                    .map_err(anyhow::Error::from)?;
            }
        }
    }

    Ok((events_processed, hours_created, days_created))
}
