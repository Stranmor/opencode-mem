//! PostgreSQL storage backend using sqlx.
//!
//! Split into modular files matching the SQLite storage pattern.

// PostgreSQL uses i64 for counts/limits, Rust uses usize - safe conversions within DB context
#![allow(
    clippy::as_conversions,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "PostgreSQL i64 <-> Rust usize conversions are safe within DB row counts"
)]
// Arithmetic in DB operations (pagination, counting) is bounded by DB limits
#![allow(
    clippy::arithmetic_side_effects,
    reason = "DB row counts and pagination are bounded by PostgreSQL limits"
)]
// Absolute paths in error handling are acceptable
#![allow(clippy::absolute_paths, reason = "std paths in error handling are clear")]

mod embeddings;
mod knowledge;
mod observations;
mod pending;
mod prompts;
mod search;
mod sessions;
mod stats;
mod summaries;

use std::collections::HashSet;

use anyhow::Result;
use chrono::{DateTime, Utc};
use opencode_mem_core::{
    GlobalKnowledge, KnowledgeType, NoiseLevel, Observation, ObservationType, SearchResult, Session,
    SessionStatus, SessionSummary, UserPrompt,
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
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .idle_timeout(std::time::Duration::from_secs(300))
            .test_before_acquire(true)
            .connect(database_url)
            .await?;
        run_pg_migrations(&pool).await?;
        tracing::info!("PgStorage initialized");
        Ok(Self { pool })
    }
}

pub(crate) fn parse_json_value<T: serde::de::DeserializeOwned>(val: &serde_json::Value) -> Vec<T> {
    serde_json::from_value(val.clone()).unwrap_or_default()
}

pub(crate) fn pg_union_dedup(existing: &[String], newer: &[String]) -> Vec<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut result = Vec::with_capacity(existing.len().saturating_add(newer.len()));
    for item in existing.iter().chain(newer.iter()) {
        if seen.insert(item.as_str()) {
            result.push(item.clone());
        }
    }
    result
}

pub(crate) fn row_to_observation(row: &sqlx::postgres::PgRow) -> Result<Observation> {
    let obs_type_str: String = row.try_get("observation_type")?;
    let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
        .unwrap_or_else(|_| obs_type_str.parse().unwrap_or_else(|_| {
            tracing::warn!(invalid_type = %obs_type_str, "corrupt observation_type in DB, defaulting to Change");
            ObservationType::Change
        }));
    let noise_str: Option<String> = row.try_get("noise_level")?;
    let noise_level = match noise_str {
        Some(s) => s.parse::<NoiseLevel>().unwrap_or_else(|_| {
            tracing::warn!(invalid_level = %s, "corrupt noise_level in DB, defaulting");
            NoiseLevel::default()
        }),
        None => NoiseLevel::default(),
    };
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
    .maybe_prompt_number(row.try_get::<Option<i32>, _>("prompt_number")?.map(|v| v as u32))
    .maybe_discovery_tokens(row.try_get::<Option<i32>, _>("discovery_tokens")?.map(|v| v as u32))
    .noise_level(noise_level)
    .maybe_noise_reason(noise_reason)
    .created_at(created_at)
    .build())
}

pub(crate) fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

pub(crate) fn row_to_search_result(row: &sqlx::postgres::PgRow) -> Result<SearchResult> {
    let obs_type_str: String = row.try_get("observation_type")?;
    let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
        .unwrap_or_else(|_| obs_type_str.parse().unwrap_or_else(|_| {
            tracing::warn!(invalid_type = %obs_type_str, "corrupt observation_type in DB, defaulting to Change");
            ObservationType::Change
        }));
    let noise_str: Option<String> = row.try_get("noise_level")?;
    let noise_level = match noise_str {
        Some(s) => s.parse::<NoiseLevel>().unwrap_or_else(|_| {
            tracing::warn!(invalid_level = %s, "corrupt noise_level in DB, defaulting");
            NoiseLevel::default()
        }),
        None => NoiseLevel::default(),
    };
    let score: f64 = row.try_get("score").unwrap_or(1.0);
    Ok(SearchResult::new(
        row.try_get("id")?,
        row.try_get("title")?,
        row.try_get("subtitle")?,
        obs_type,
        noise_level,
        score,
    ))
}

pub(crate) fn row_to_search_result_default(row: &sqlx::postgres::PgRow) -> Result<SearchResult> {
    let obs_type_str: String = row.try_get("observation_type")?;
    let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
        .unwrap_or_else(|_| obs_type_str.parse().unwrap_or_else(|_| {
            tracing::warn!(invalid_type = %obs_type_str, "corrupt observation_type in DB, defaulting to Change");
            ObservationType::Change
        }));
    let noise_str: Option<String> = row.try_get("noise_level")?;
    let noise_level = match noise_str {
        Some(s) => s.parse::<NoiseLevel>().unwrap_or_else(|_| {
            tracing::warn!(invalid_level = %s, "corrupt noise_level in DB, defaulting");
            NoiseLevel::default()
        }),
        None => NoiseLevel::default(),
    };
    Ok(SearchResult::new(
        row.try_get("id")?,
        row.try_get("title")?,
        row.try_get("subtitle")?,
        obs_type,
        noise_level,
        1.0,
    ))
}

pub(crate) fn row_to_session(row: &sqlx::postgres::PgRow) -> Result<Session> {
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
        row.try_get::<i32, _>("prompt_counter")? as u32,
    ))
}

pub(crate) fn row_to_summary(row: &sqlx::postgres::PgRow) -> Result<SessionSummary> {
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
        row.try_get::<Option<i32>, _>("prompt_number")?.map(|v| v as u32),
        row.try_get::<Option<i32>, _>("discovery_tokens")?.map(|v| v as u32),
        created_at,
    ))
}

pub(crate) fn row_to_knowledge(row: &sqlx::postgres::PgRow) -> Result<GlobalKnowledge> {
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
        i64::from(row.try_get::<i32, _>("usage_count")?),
        last_used_at.map(|d| d.to_rfc3339()),
        created_at.to_rfc3339(),
        updated_at.to_rfc3339(),
    ))
}

pub(crate) fn row_to_prompt(row: &sqlx::postgres::PgRow) -> Result<UserPrompt> {
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    Ok(UserPrompt::new(
        row.try_get("id")?,
        row.try_get("content_session_id")?,
        row.try_get::<i32, _>("prompt_number")? as u32,
        row.try_get("prompt_text")?,
        row.try_get("project")?,
        created_at,
    ))
}

pub(crate) fn row_to_pending_message(row: &sqlx::postgres::PgRow) -> Result<PendingMessage> {
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
        .map(|w| format!("{}:*", w.replace('\'', "")))
        .collect::<Vec<_>>()
        .join(" & ")
}

pub(crate) const SESSION_COLUMNS: &str = "id, content_session_id, memory_session_id, project, user_prompt,
     started_at, ended_at, status, prompt_counter";

pub(crate) const KNOWLEDGE_COLUMNS: &str = "id, knowledge_type, title, description, instructions, triggers,
     source_projects, source_observations, confidence, usage_count,
     last_used_at, created_at, updated_at";

pub(crate) const SUMMARY_COLUMNS: &str =
    "session_id, project, request, investigated, learned, completed, next_steps, notes,
     files_read, files_edited, prompt_number, discovery_tokens, created_at";
