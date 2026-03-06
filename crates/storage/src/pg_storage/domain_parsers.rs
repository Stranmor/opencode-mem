//! Domain-specific row-to-type parsers for sessions, summaries, knowledge, prompts, and pending messages.

use chrono::{DateTime, Utc};
use opencode_mem_core::{
    ContentSessionId, DiscoveryTokens, GlobalKnowledge, KnowledgeType, ProjectId, PromptNumber,
    Session, SessionId, SessionStatus, SessionSummary, UserPrompt,
};
use sqlx::Row;

use crate::error::StorageError;
use crate::pending_queue::{PendingMessage, PendingMessageStatus};

use super::row_parsers::parse_json_value;

pub(crate) fn row_to_session(row: &sqlx::postgres::PgRow) -> Result<Session, StorageError> {
    let started_at: DateTime<Utc> = row.try_get("started_at")?;
    let ended_at: Option<DateTime<Utc>> = row.try_get("ended_at")?;
    let status_str: String = row.try_get("status")?;
    let status: SessionStatus = status_str
        .parse()
        .or_else(|_| serde_json::from_str::<SessionStatus>(&status_str))
        .map_err(|e| StorageError::DataCorruption {
            context: format!("invalid session status in DB: {}", status_str),
            source: Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()),
        })?;
    Ok(Session::new(
        row.try_get::<SessionId, _>("id")?,
        row.try_get::<ContentSessionId, _>("content_session_id")?,
        row.try_get("memory_session_id")?,
        row.try_get::<ProjectId, _>("project")?,
        row.try_get("user_prompt")?,
        started_at,
        ended_at,
        status,
        u32::try_from(row.try_get::<i32, _>("prompt_counter")?).map_err(|e| {
            StorageError::DataCorruption {
                context: "prompt_counter negative in DB".into(),
                source: Box::new(e),
            }
        })?,
    ))
}

pub(crate) fn row_to_summary(row: &sqlx::postgres::PgRow) -> Result<SessionSummary, StorageError> {
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let files_read: serde_json::Value = row.try_get("files_read")?;
    let files_edited: serde_json::Value = row.try_get("files_edited")?;
    Ok(SessionSummary::new(
        row.try_get::<SessionId, _>("session_id")?,
        row.try_get::<ProjectId, _>("project")?,
        row.try_get("request")?,
        row.try_get("investigated")?,
        row.try_get("learned")?,
        row.try_get("completed")?,
        row.try_get("next_steps")?,
        row.try_get("notes")?,
        parse_json_value(files_read),
        parse_json_value(files_edited),
        row.try_get::<Option<i32>, _>("prompt_number")?
            .map(|v| {
                u32::try_from(v).map_err(|e| StorageError::DataCorruption {
                    context: "prompt_number negative in DB".into(),
                    source: Box::new(e),
                })
            })
            .transpose()?
            .map(PromptNumber),
        row.try_get::<Option<i32>, _>("discovery_tokens")?
            .map(|v| {
                u32::try_from(v).map_err(|e| StorageError::DataCorruption {
                    context: "discovery_tokens negative in DB".into(),
                    source: Box::new(e),
                })
            })
            .transpose()?
            .map(DiscoveryTokens),
        created_at,
    ))
}

pub(crate) fn row_to_knowledge(
    row: &sqlx::postgres::PgRow,
) -> Result<GlobalKnowledge, StorageError> {
    let kt_str: String = row.try_get("knowledge_type")?;
    let knowledge_type: KnowledgeType =
        kt_str
            .parse::<KnowledgeType>()
            .map_err(|e| StorageError::DataCorruption {
                context: format!("invalid knowledge_type in DB: {}", kt_str),
                source: Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()),
            })?;
    let triggers: serde_json::Value = row.try_get("triggers")?;
    let source_projects: serde_json::Value = row.try_get("source_projects")?;
    let source_observations: serde_json::Value = row.try_get("source_observations")?;
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
    let last_used_at: Option<DateTime<Utc>> = row.try_get("last_used_at")?;
    let archived_at: Option<DateTime<Utc>> = row.try_get("archived_at")?;
    Ok(GlobalKnowledge::new(
        row.try_get("id")?,
        knowledge_type,
        row.try_get("title")?,
        row.try_get("description")?,
        row.try_get("instructions")?,
        parse_json_value(triggers),
        parse_json_value(source_projects),
        parse_json_value(source_observations),
        row.try_get("confidence")?,
        row.try_get::<i64, _>("usage_count")?,
        last_used_at.map(|d| d.to_rfc3339()),
        created_at.to_rfc3339(),
        updated_at.to_rfc3339(),
        archived_at.map(|d| d.to_rfc3339()),
    ))
}

pub(crate) fn row_to_prompt(row: &sqlx::postgres::PgRow) -> Result<UserPrompt, StorageError> {
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    Ok(UserPrompt::new(
        row.try_get("id")?,
        row.try_get::<ContentSessionId, _>("content_session_id")?,
        PromptNumber(
            u32::try_from(row.try_get::<i32, _>("prompt_number")?).map_err(|e| {
                StorageError::DataCorruption {
                    context: "prompt_number negative in DB".into(),
                    source: Box::new(e),
                }
            })?,
        ),
        row.try_get("prompt_text")?,
        row.try_get::<Option<ProjectId>, _>("project")?,
        created_at,
    ))
}

pub(crate) fn row_to_pending_message(
    row: &sqlx::postgres::PgRow,
) -> Result<PendingMessage, StorageError> {
    let status_str: String = row.try_get("status")?;
    let status =
        status_str
            .parse::<PendingMessageStatus>()
            .map_err(|e| StorageError::DataCorruption {
                context: format!("invalid pending message status in DB: {}", status_str),
                source: Box::<dyn std::error::Error + Send + Sync>::from(e.to_string()),
            })?;
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

pub(crate) fn collect_skipping_corrupt<T>(
    results: impl Iterator<Item = Result<T, StorageError>>,
) -> Vec<T> {
    results
        .filter_map(|r| match r {
            Ok(val) => Some(val),
            Err(StorageError::DataCorruption { ref context, .. }) => {
                tracing::warn!("Skipping corrupt row: {context}");
                None
            }
            Err(e) => {
                tracing::warn!("Skipping row with unexpected error: {e}");
                None
            }
        })
        .collect()
}
