//! PostgreSQL storage backend using sqlx.
//!
//! Split into modular files by domain concern.

// Arithmetic in DB operations (pagination, counting) is bounded by DB limits
#![allow(
    clippy::arithmetic_side_effects,
    reason = "DB row counts and pagination are bounded by PostgreSQL limits"
)]
// Absolute paths in error handling are acceptable
#![allow(clippy::absolute_paths, reason = "std paths in error handling are clear")]

mod embeddings;
mod injections;
mod knowledge;
mod observations;
mod pending;
mod prompts;
mod search;
mod sessions;
mod stats;
mod summaries;

use std::collections::HashSet;

use crate::error::StorageError;
use chrono::{DateTime, Utc};
use opencode_mem_core::{
    sort_by_score_descending, DiscoveryTokens, GlobalKnowledge, KnowledgeType, NoiseLevel,
    Observation, ObservationType, PromptNumber, SearchResult, Session, SessionStatus,
    SessionSummary, UserPrompt, PG_POOL_ACQUIRE_TIMEOUT_SECS, PG_POOL_IDLE_TIMEOUT_SECS,
    PG_POOL_MAX_CONNECTIONS,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

use crate::pending_queue::{PendingMessage, PendingMessageStatus};

use super::pg_migrations::run_pg_migrations;

#[derive(Clone, Debug)]
pub struct PgStorage {
    pool: PgPool,
}

impl PgStorage {
    pub async fn new(database_url: &str) -> Result<Self, StorageError> {
        let pool = PgPoolOptions::new()
            .max_connections(PG_POOL_MAX_CONNECTIONS)
            .acquire_timeout(std::time::Duration::from_secs(PG_POOL_ACQUIRE_TIMEOUT_SECS))
            .idle_timeout(std::time::Duration::from_secs(PG_POOL_IDLE_TIMEOUT_SECS))
            .test_before_acquire(true)
            .connect(database_url)
            .await?;
        run_pg_migrations(&pool).await.map_err(|e| StorageError::Migration(e.to_string()))?;
        tracing::info!("PgStorage initialized");
        Ok(Self { pool })
    }
}

pub(crate) fn parse_json_value<T: serde::de::DeserializeOwned>(val: &serde_json::Value) -> Vec<T> {
    serde_json::from_value(val.clone()).unwrap_or_default()
}

/// Parse `ObservationType` from a PostgreSQL text column.
/// Tries JSON deserialization first (handles quoted values), then `FromStr`.
pub(crate) fn parse_pg_observation_type(s: &str) -> ObservationType {
    serde_json::from_str(s)
        .unwrap_or_else(|_| s.parse().unwrap_or_else(|_| {
            tracing::warn!(invalid_type = %s, "corrupt observation_type in DB, defaulting to Change");
            ObservationType::Change
        }))
}

/// Parse `NoiseLevel` from an optional PostgreSQL text column.
pub(crate) fn parse_pg_noise_level(s: Option<&str>) -> NoiseLevel {
    match s {
        Some(s) => s.parse::<NoiseLevel>().unwrap_or_else(|_| {
            tracing::warn!(invalid_level = %s, "corrupt noise_level in DB, defaulting");
            NoiseLevel::default()
        }),
        None => NoiseLevel::default(),
    }
}

pub(crate) fn row_to_observation(row: &sqlx::postgres::PgRow) -> Result<Observation, StorageError> {
    let obs_type = parse_pg_observation_type(&row.try_get::<String, _>("observation_type")?);
    let noise_level =
        parse_pg_noise_level(row.try_get::<Option<String>, _>("noise_level")?.as_deref());
    let noise_reason: Option<String> = row.try_get("noise_reason")?;
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let facts: serde_json::Value = row.try_get("facts")?;
    let concepts: serde_json::Value = row.try_get("concepts")?;
    let files_read: serde_json::Value = row.try_get("files_read")?;
    let files_modified: serde_json::Value = row.try_get("files_modified")?;
    let keywords: serde_json::Value = row.try_get("keywords")?;

    Ok(Observation::builder(
        row.try_get("id")?,
        row.try_get("session_id")?,
        obs_type,
        row.try_get("title")?,
    )
    .maybe_project(row.try_get("project")?)
    .maybe_subtitle(row.try_get("subtitle")?)
    .maybe_narrative(row.try_get("narrative")?)
    .facts(parse_json_value(&facts))
    .concepts(parse_json_value(&concepts))
    .files_read(parse_json_value(&files_read))
    .files_modified(parse_json_value(&files_modified))
    .keywords(parse_json_value(&keywords))
    .maybe_prompt_number(
        row.try_get::<Option<i32>, _>("prompt_number")?
            .map(|v| PromptNumber(u32::try_from(v).unwrap_or(0))),
    )
    .maybe_discovery_tokens(
        row.try_get::<Option<i32>, _>("discovery_tokens")?
            .map(|v| DiscoveryTokens(u32::try_from(v).unwrap_or(0))),
    )
    .noise_level(noise_level)
    .maybe_noise_reason(noise_reason)
    .created_at(created_at)
    .build())
}

pub(crate) fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// Convert `usize` to `i64` for SQL LIMIT/OFFSET binds.
/// Saturates to `i64::MAX` on overflow (only possible on 128-bit targets).
pub(crate) fn usize_to_i64(val: usize) -> i64 {
    i64::try_from(val).unwrap_or(i64::MAX)
}

pub(crate) fn row_to_search_result(
    row: &sqlx::postgres::PgRow,
) -> Result<SearchResult, StorageError> {
    let obs_type = parse_pg_observation_type(&row.try_get::<String, _>("observation_type")?);
    let noise_level =
        parse_pg_noise_level(row.try_get::<Option<String>, _>("noise_level")?.as_deref());
    let score: f64 = row.try_get("score").unwrap_or(0.0);
    Ok(SearchResult::new(
        row.try_get("id")?,
        row.try_get("title")?,
        row.try_get("subtitle")?,
        obs_type,
        noise_level,
        score,
    ))
}

pub(crate) fn row_to_search_result_with_score(
    row: &sqlx::postgres::PgRow,
    score: f64,
) -> Result<SearchResult, StorageError> {
    let obs_type = parse_pg_observation_type(&row.try_get::<String, _>("observation_type")?);
    let noise_level =
        parse_pg_noise_level(row.try_get::<Option<String>, _>("noise_level")?.as_deref());
    Ok(SearchResult::new(
        row.try_get("id")?,
        row.try_get("title")?,
        row.try_get("subtitle")?,
        obs_type,
        noise_level,
        score,
    ))
}

pub(crate) fn row_to_session(row: &sqlx::postgres::PgRow) -> Result<Session, StorageError> {
    let started_at: DateTime<Utc> = row.try_get("started_at")?;
    let ended_at: Option<DateTime<Utc>> = row.try_get("ended_at")?;
    let status_str: String = row.try_get("status")?;
    let status: SessionStatus = status_str
        .parse()
        .or_else(|_| serde_json::from_str::<SessionStatus>(&status_str))
        .unwrap_or_else(|_| {
            tracing::warn!(invalid_status = %status_str, "corrupt session status in DB, defaulting to Active");
            SessionStatus::Active
        });
    Ok(Session::new(
        row.try_get("id")?,
        row.try_get("content_session_id")?,
        row.try_get("memory_session_id")?,
        row.try_get("project")?,
        row.try_get("user_prompt")?,
        started_at,
        ended_at,
        status,
        u32::try_from(row.try_get::<i32, _>("prompt_counter")?).unwrap_or(0),
    ))
}

pub(crate) fn row_to_summary(row: &sqlx::postgres::PgRow) -> Result<SessionSummary, StorageError> {
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let files_read: serde_json::Value = row.try_get("files_read")?;
    let files_edited: serde_json::Value = row.try_get("files_edited")?;
    Ok(SessionSummary::new(
        row.try_get("session_id")?,
        row.try_get("project")?,
        row.try_get("request")?,
        row.try_get("investigated")?,
        row.try_get("learned")?,
        row.try_get("completed")?,
        row.try_get("next_steps")?,
        row.try_get("notes")?,
        parse_json_value(&files_read),
        parse_json_value(&files_edited),
        row.try_get::<Option<i32>, _>("prompt_number")?
            .map(|v| PromptNumber(u32::try_from(v).unwrap_or(0))),
        row.try_get::<Option<i32>, _>("discovery_tokens")?
            .map(|v| DiscoveryTokens(u32::try_from(v).unwrap_or(0))),
        created_at,
    ))
}

pub(crate) fn row_to_knowledge(
    row: &sqlx::postgres::PgRow,
) -> Result<GlobalKnowledge, StorageError> {
    let kt_str: String = row.try_get("knowledge_type")?;
    let knowledge_type: KnowledgeType = kt_str.parse().unwrap_or_else(|_| {
        tracing::warn!(invalid_type = %kt_str, "corrupt knowledge_type in DB, defaulting to Pattern");
        KnowledgeType::Pattern
    });
    let triggers: serde_json::Value = row.try_get("triggers")?;
    let source_projects: serde_json::Value = row.try_get("source_projects")?;
    let source_observations: serde_json::Value = row.try_get("source_observations")?;
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
    let last_used_at: Option<DateTime<Utc>> = row.try_get("last_used_at")?;
    Ok(GlobalKnowledge::new(
        row.try_get("id")?,
        knowledge_type,
        row.try_get("title")?,
        row.try_get("description")?,
        row.try_get("instructions")?,
        parse_json_value(&triggers),
        parse_json_value(&source_projects),
        parse_json_value(&source_observations),
        row.try_get("confidence")?,
        row.try_get::<i64, _>("usage_count")?,
        last_used_at.map(|d| d.to_rfc3339()),
        created_at.to_rfc3339(),
        updated_at.to_rfc3339(),
    ))
}

pub(crate) fn row_to_prompt(row: &sqlx::postgres::PgRow) -> Result<UserPrompt, StorageError> {
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    Ok(UserPrompt::new(
        row.try_get("id")?,
        row.try_get("content_session_id")?,
        PromptNumber(u32::try_from(row.try_get::<i32, _>("prompt_number")?).unwrap_or(0)),
        row.try_get("prompt_text")?,
        row.try_get("project")?,
        created_at,
    ))
}

pub(crate) fn row_to_pending_message(
    row: &sqlx::postgres::PgRow,
) -> Result<PendingMessage, StorageError> {
    let status_str: String = row.try_get("status")?;
    let status =
        status_str.parse::<PendingMessageStatus>().unwrap_or_else(|_| {
            tracing::warn!(invalid_status = %status_str, "corrupt pending message status in DB, defaulting to Pending");
            PendingMessageStatus::Pending
        });
    Ok(PendingMessage {
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        status,
        tool_name: row.try_get("tool_name")?,
        tool_input: row.try_get("tool_input")?,
        tool_response: row.try_get("tool_response")?,
        retry_count: row.try_get("retry_count")?,
        created_at_epoch: row.try_get("created_at_epoch")?,
        claimed_at_epoch: row.try_get("claimed_at_epoch")?,
        completed_at_epoch: row.try_get("completed_at_epoch")?,
        project: row.try_get("project")?,
    })
}

pub(crate) fn build_tsquery(query: &str) -> String {
    query
        .split_whitespace()
        .filter_map(|w| {
            // Strip tsquery operators and special characters, keep only alphanumeric
            let sanitized: String =
                w.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_').collect();
            if sanitized.is_empty() {
                None
            } else {
                Some(format!("{}:*", sanitized))
            }
        })
        .collect::<Vec<_>>()
        .join(" & ")
}

pub(crate) const SESSION_COLUMNS: &str =
    "id, content_session_id, memory_session_id, project, user_prompt,
     started_at, ended_at, status, prompt_counter";

pub(crate) const KNOWLEDGE_COLUMNS: &str =
    "id, knowledge_type, title, description, instructions, triggers,
     source_projects, source_observations, confidence, usage_count,
     last_used_at, created_at, updated_at";

pub(crate) const SUMMARY_COLUMNS: &str =
    "session_id, project, request, investigated, learned, completed, next_steps, notes,
     files_read, files_edited, prompt_number, discovery_tokens, created_at";
