//! Core row-to-domain type conversion functions for PostgreSQL query results.
//!
//! Domain-specific parsers (sessions, summaries, knowledge, etc.) are in `domain_parsers`.

use chrono::{DateTime, Utc};
use opencode_mem_core::{
    DiscoveryTokens, NoiseLevel, Observation, ObservationId, ObservationType, ProjectId,
    PromptNumber, SearchResult, SessionId,
};
use sqlx::Row;

use crate::error::StorageError;

pub(crate) fn parse_json_value<T: serde::de::DeserializeOwned>(
    val: serde_json::Value,
    context: &str,
) -> Result<Vec<T>, StorageError> {
    serde_json::from_value(val).map_err(|e| {
        tracing::warn!("Failed to parse JSON column '{}': {}", context, e);
        StorageError::DataCorruption {
            context: format!("invalid JSON in DB column: '{context}'"),
            source: Box::new(e),
        }
    })
}

pub(crate) fn parse_pg_observation_type(s: &str) -> Result<ObservationType, StorageError> {
    serde_json::from_str(s)
        .or_else(|_| s.parse::<ObservationType>())
        .map_err(|e| {
            tracing::warn!("Invalid observation_type '{}' in DB: {}", s, e);
            StorageError::DataCorruption {
                context: format!("invalid observation_type in DB: '{s}'"),
                source: Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()),
            }
        })
}

pub(crate) fn parse_pg_noise_level(s: Option<&str>) -> Result<NoiseLevel, StorageError> {
    match s {
        Some(s) => s.parse::<NoiseLevel>().map_err(|e| {
            tracing::warn!("Invalid noise_level '{}' in DB: {}", s, e);
            StorageError::DataCorruption {
                context: format!("invalid noise_level in DB: '{s}'"),
                source: Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()),
            }
        }),
        None => Ok(NoiseLevel::Medium),
    }
}

pub(crate) fn row_to_observation(row: &sqlx::postgres::PgRow) -> Result<Observation, StorageError> {
    let obs_type = parse_pg_observation_type(&row.try_get::<String, _>("observation_type")?)?;
    let noise_level =
        parse_pg_noise_level(row.try_get::<Option<String>, _>("noise_level")?.as_deref())?;
    let noise_reason: Option<String> = row.try_get("noise_reason")?;
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let facts: serde_json::Value = row.try_get("facts")?;
    let concepts: serde_json::Value = row.try_get("concepts")?;
    let files_read: serde_json::Value = row.try_get("files_read")?;
    let files_modified: serde_json::Value = row.try_get("files_modified")?;
    let keywords: serde_json::Value = row.try_get("keywords")?;

    Ok(Observation::builder(
        row.try_get::<ObservationId, _>("id")?,
        row.try_get::<SessionId, _>("session_id")?,
        obs_type,
        row.try_get("title")?,
    )
    .maybe_project(row.try_get::<Option<ProjectId>, _>("project")?)
    .maybe_subtitle(row.try_get("subtitle")?)
    .maybe_narrative(row.try_get("narrative")?)
    .facts(parse_json_value(facts, "facts")?)
    .concepts(parse_json_value(concepts, "concepts")?)
    .files_read(parse_json_value(files_read, "files_read")?)
    .files_modified(parse_json_value(files_modified, "files_modified")?)
    .keywords(parse_json_value(keywords, "keywords")?)
    .maybe_prompt_number(
        row.try_get::<Option<i32>, _>("prompt_number")?
            .map(|v| {
                u32::try_from(v).map_err(|e| StorageError::DataCorruption {
                    context: "prompt_number negative in DB".into(),
                    source: Box::new(e),
                })
            })
            .transpose()?
            .map(PromptNumber),
    )
    .maybe_discovery_tokens(
        row.try_get::<Option<i32>, _>("discovery_tokens")?
            .map(|v| {
                u32::try_from(v).map_err(|e| StorageError::DataCorruption {
                    context: "discovery_tokens negative in DB".into(),
                    source: Box::new(e),
                })
            })
            .transpose()?
            .map(DiscoveryTokens),
    )
    .noise_level(noise_level)
    .maybe_noise_reason(noise_reason)
    .created_at(created_at)
    .build())
}

pub(crate) fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub(crate) fn usize_to_i64(val: usize) -> i64 {
    i64::try_from(val).unwrap_or(i64::MAX)
}

pub(crate) fn row_to_search_result(
    row: &sqlx::postgres::PgRow,
) -> Result<SearchResult, StorageError> {
    let obs_type = parse_pg_observation_type(&row.try_get::<String, _>("observation_type")?)?;
    let noise_level =
        parse_pg_noise_level(row.try_get::<Option<String>, _>("noise_level")?.as_deref())?;
    let score: f64 = row.try_get("score").unwrap_or(0.0);
    Ok(SearchResult::new(
        row.try_get::<ObservationId, _>("id")?,
        row.try_get("title")?,
        row.try_get("subtitle")?,
        obs_type,
        noise_level,
        score,
    ))
}

#[allow(
    dead_code,
    reason = "Legacy fallback method for backward compatibility"
)]
pub(crate) fn row_to_search_result_with_score(
    row: &sqlx::postgres::PgRow,
    score: f64,
) -> Result<SearchResult, StorageError> {
    let obs_type = parse_pg_observation_type(&row.try_get::<String, _>("observation_type")?)?;
    let noise_level =
        parse_pg_noise_level(row.try_get::<Option<String>, _>("noise_level")?.as_deref())?;
    Ok(SearchResult::new(
        row.try_get::<ObservationId, _>("id")?,
        row.try_get("title")?,
        row.try_get("subtitle")?,
        obs_type,
        noise_level,
        score,
    ))
}
