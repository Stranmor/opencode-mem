//! PostgreSQL storage backend using sqlx.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use opencode_mem_core::{
    GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType, NoiseLevel, Observation,
    ObservationType, SearchResult, Session, SessionStatus, SessionSummary, UserPrompt,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

use crate::pending_queue::{
    max_retry_count, PaginatedResult, PendingMessage, PendingMessageStatus, QueueStats,
    StorageStats,
};
use crate::traits::{
    EmbeddingStore, KnowledgeStore, ObservationStore, PendingQueueStore, PromptStore, SearchStore,
    SessionStore, StatsStore, SummaryStore,
};

use super::pg_migrations::run_pg_migrations;

#[derive(Clone, Debug)]
pub struct PgStorage {
    pool: PgPool,
}

impl PgStorage {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new().max_connections(8).connect(database_url).await?;
        run_pg_migrations(&pool).await?;
        tracing::info!("PgStorage initialized");
        Ok(Self { pool })
    }
}

fn parse_json_value<T: serde::de::DeserializeOwned>(val: &serde_json::Value) -> Vec<T> {
    serde_json::from_value(val.clone()).unwrap_or_default()
}

fn row_to_observation(row: &sqlx::postgres::PgRow) -> Result<Observation> {
    let obs_type_str: String = row.try_get("observation_type")?;
    let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
        .unwrap_or_else(|_| obs_type_str.parse().unwrap_or(ObservationType::Change));
    let noise_str: Option<String> = row.try_get("noise_level")?;
    let noise_level = noise_str.and_then(|s| s.parse::<NoiseLevel>().ok()).unwrap_or_default();
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

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

fn row_to_search_result(row: &sqlx::postgres::PgRow) -> Result<SearchResult> {
    let obs_type_str: String = row.try_get("observation_type")?;
    let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
        .unwrap_or_else(|_| obs_type_str.parse().unwrap_or(ObservationType::Change));
    let noise_str: Option<String> = row.try_get("noise_level")?;
    let noise_level = noise_str.and_then(|s| s.parse::<NoiseLevel>().ok()).unwrap_or_default();
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

fn row_to_search_result_default(row: &sqlx::postgres::PgRow) -> Result<SearchResult> {
    let obs_type_str: String = row.try_get("observation_type")?;
    let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
        .unwrap_or_else(|_| obs_type_str.parse().unwrap_or(ObservationType::Change));
    let noise_str: Option<String> = row.try_get("noise_level")?;
    let noise_level = noise_str.and_then(|s| s.parse::<NoiseLevel>().ok()).unwrap_or_default();
    Ok(SearchResult::new(
        row.try_get("id")?,
        row.try_get("title")?,
        row.try_get("subtitle")?,
        obs_type,
        noise_level,
        1.0,
    ))
}

fn row_to_session(row: &sqlx::postgres::PgRow) -> Result<Session> {
    let started_at: DateTime<Utc> = row.try_get("started_at")?;
    let ended_at: Option<DateTime<Utc>> = row.try_get("ended_at")?;
    let status_str: String = row.try_get("status")?;
    let status: SessionStatus = status_str
        .parse()
        .or_else(|_| serde_json::from_str::<SessionStatus>(&status_str))
        .unwrap_or(SessionStatus::Active);
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

fn row_to_summary(row: &sqlx::postgres::PgRow) -> Result<SessionSummary> {
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

fn row_to_knowledge(row: &sqlx::postgres::PgRow) -> Result<GlobalKnowledge> {
    let kt_str: String = row.try_get("knowledge_type")?;
    let knowledge_type: KnowledgeType = kt_str.parse().unwrap_or(KnowledgeType::Pattern);
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

fn row_to_prompt(row: &sqlx::postgres::PgRow) -> Result<UserPrompt> {
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

fn row_to_pending_message(row: &sqlx::postgres::PgRow) -> Result<PendingMessage> {
    let status_str: String = row.try_get("status")?;
    let status =
        status_str.parse::<PendingMessageStatus>().unwrap_or(PendingMessageStatus::Pending);
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

fn build_tsquery(query: &str) -> String {
    query
        .split_whitespace()
        .map(|w| format!("{}:*", w.replace('\'', "")))
        .collect::<Vec<_>>()
        .join(" & ")
}

const SESSION_COLUMNS: &str = "id, content_session_id, memory_session_id, project, user_prompt,
     started_at, ended_at, status, prompt_counter";

const KNOWLEDGE_COLUMNS: &str = "id, knowledge_type, title, description, instructions, triggers,
     source_projects, source_observations, confidence, usage_count,
     last_used_at, created_at, updated_at";

const SUMMARY_COLUMNS: &str =
    "session_id, project, request, investigated, learned, completed, next_steps, notes,
     files_read, files_edited, prompt_number, discovery_tokens, created_at";

// ── ObservationStore ─────────────────────────────────────────────

#[async_trait]
impl ObservationStore for PgStorage {
    async fn save_observation(&self, obs: &Observation) -> Result<bool> {
        let result = sqlx::query(
            r#"INSERT INTO observations
               (id, session_id, project, observation_type, title, subtitle, narrative,
                facts, concepts, files_read, files_modified, keywords,
                prompt_number, discovery_tokens, noise_level, noise_reason, created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
               ON CONFLICT (id) DO NOTHING"#,
        )
        .bind(&obs.id)
        .bind(&obs.session_id)
        .bind(&obs.project)
        .bind(obs.observation_type.as_str())
        .bind(&obs.title)
        .bind(&obs.subtitle)
        .bind(&obs.narrative)
        .bind(serde_json::to_value(&obs.facts)?)
        .bind(serde_json::to_value(&obs.concepts)?)
        .bind(serde_json::to_value(&obs.files_read)?)
        .bind(serde_json::to_value(&obs.files_modified)?)
        .bind(serde_json::to_value(&obs.keywords)?)
        .bind(obs.prompt_number.map(|v| v as i32))
        .bind(obs.discovery_tokens.map(|v| v as i32))
        .bind(obs.noise_level.as_str())
        .bind(&obs.noise_reason)
        .bind(obs.created_at)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<Observation>> {
        let row = sqlx::query(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                    facts, concepts, files_read, files_modified, keywords,
                    prompt_number, discovery_tokens, noise_level, noise_reason, created_at
             FROM observations WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| row_to_observation(&r)).transpose()
    }

    async fn get_recent(&self, limit: usize) -> Result<Vec<SearchResult>> {
        let rows = sqlx::query(
            "SELECT id, title, subtitle, observation_type, noise_level, 1.0::float8 as score
             FROM observations ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_search_result).collect()
    }

    async fn get_session_observations(&self, session_id: &str) -> Result<Vec<Observation>> {
        let rows = sqlx::query(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                    facts, concepts, files_read, files_modified, keywords,
                    prompt_number, discovery_tokens, noise_level, noise_reason, created_at
             FROM observations WHERE session_id = $1 ORDER BY created_at",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_observation).collect()
    }

    async fn get_observations_by_ids(&self, ids: &[String]) -> Result<Vec<Observation>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                    facts, concepts, files_read, files_modified, keywords,
                    prompt_number, discovery_tokens, noise_level, noise_reason, created_at
             FROM observations WHERE id = ANY($1) ORDER BY created_at DESC",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_observation).collect()
    }

    async fn get_context_for_project(
        &self,
        project: &str,
        limit: usize,
    ) -> Result<Vec<Observation>> {
        let rows = sqlx::query(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                    facts, concepts, files_read, files_modified, keywords,
                    prompt_number, discovery_tokens, noise_level, noise_reason, created_at
             FROM observations WHERE project = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(project)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_observation).collect()
    }

    async fn get_session_observation_count(&self, session_id: &str) -> Result<usize> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM observations WHERE session_id = $1")
                .bind(session_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(count as usize)
    }

    async fn search_by_file(&self, file_path: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let pattern = format!("%{}%", escape_like(file_path));
        let rows = sqlx::query(
            r#"SELECT id, title, subtitle, observation_type, noise_level, 1.0::float8 as score
               FROM observations
               WHERE files_read::text LIKE $1 OR files_modified::text LIKE $1
               ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(&pattern)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_search_result).collect()
    }
}

// ── SessionStore ─────────────────────────────────────────────────

#[async_trait]
impl SessionStore for PgStorage {
    async fn save_session(&self, session: &Session) -> Result<()> {
        sqlx::query(&format!(
            "INSERT INTO sessions ({SESSION_COLUMNS})
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (id) DO UPDATE SET
               content_session_id = EXCLUDED.content_session_id,
               memory_session_id = EXCLUDED.memory_session_id,
               project = EXCLUDED.project,
               user_prompt = EXCLUDED.user_prompt,
               started_at = EXCLUDED.started_at,
               ended_at = EXCLUDED.ended_at,
               status = EXCLUDED.status,
               prompt_counter = EXCLUDED.prompt_counter"
        ))
        .bind(&session.id)
        .bind(&session.content_session_id)
        .bind(&session.memory_session_id)
        .bind(&session.project)
        .bind(&session.user_prompt)
        .bind(session.started_at)
        .bind(session.ended_at)
        .bind(session.status.as_str())
        .bind(session.prompt_counter as i32)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let row = sqlx::query(&format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|r| row_to_session(&r)).transpose()
    }

    async fn get_session_by_content_id(&self, content_session_id: &str) -> Result<Option<Session>> {
        let row = sqlx::query(&format!(
            "SELECT {SESSION_COLUMNS} FROM sessions WHERE content_session_id = $1"
        ))
        .bind(content_session_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| row_to_session(&r)).transpose()
    }

    async fn update_session_status(&self, id: &str, status: SessionStatus) -> Result<()> {
        let ended_at: Option<DateTime<Utc>> = (status != SessionStatus::Active).then(Utc::now);
        sqlx::query("UPDATE sessions SET status = $1, ended_at = $2 WHERE id = $3")
            .bind(status.as_str())
            .bind(ended_at)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_session(&self, session_id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM sessions WHERE id = $1")
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn close_stale_sessions(&self, max_age_hours: i64) -> Result<usize> {
        let now = Utc::now();
        let threshold = now - chrono::Duration::hours(max_age_hours);
        let result = sqlx::query(
            "UPDATE sessions SET status = $1, ended_at = $2
             WHERE status = $3 AND started_at < $4",
        )
        .bind(SessionStatus::Completed.as_str())
        .bind(now)
        .bind(SessionStatus::Active.as_str())
        .bind(threshold)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as usize)
    }
}

// ── KnowledgeStore ───────────────────────────────────────────────

#[async_trait]
impl KnowledgeStore for PgStorage {
    async fn save_knowledge(&self, input: KnowledgeInput) -> Result<GlobalKnowledge> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();

        let existing: Option<(
            String,
            DateTime<Utc>,
            serde_json::Value,
            serde_json::Value,
            serde_json::Value,
            f64,
            i32,
            Option<DateTime<Utc>>,
        )> = sqlx::query_as(
            "SELECT id, created_at, triggers, source_projects, source_observations,
                        confidence, usage_count, last_used_at
                 FROM global_knowledge
                 WHERE LOWER(TRIM(title)) = LOWER(TRIM($1))",
        )
        .bind(&input.title)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some((
            id,
            created_at,
            triggers_json,
            src_proj_json,
            src_obs_json,
            confidence,
            usage_count,
            last_used_at,
        )) = existing
        {
            let mut triggers: Vec<String> = parse_json_value(&triggers_json);
            let mut source_projects: Vec<String> = parse_json_value(&src_proj_json);
            let mut source_observations: Vec<String> = parse_json_value(&src_obs_json);

            for t in &input.triggers {
                if !triggers.contains(t) {
                    triggers.push(t.clone());
                }
            }
            if let Some(ref p) = input.source_project {
                if !source_projects.contains(p) {
                    source_projects.push(p.clone());
                }
            }
            if let Some(ref o) = input.source_observation {
                if !source_observations.contains(o) {
                    source_observations.push(o.clone());
                }
            }

            sqlx::query(
                "UPDATE global_knowledge
                 SET knowledge_type = $1, description = $2, instructions = $3,
                     triggers = $4, source_projects = $5, source_observations = $6,
                     updated_at = $7
                 WHERE id = $8",
            )
            .bind(input.knowledge_type.as_str())
            .bind(&input.description)
            .bind(&input.instructions)
            .bind(serde_json::to_value(&triggers)?)
            .bind(serde_json::to_value(&source_projects)?)
            .bind(serde_json::to_value(&source_observations)?)
            .bind(now)
            .bind(&id)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;

            Ok(GlobalKnowledge::new(
                id,
                input.knowledge_type,
                input.title,
                input.description,
                input.instructions,
                triggers,
                source_projects,
                source_observations,
                confidence,
                i64::from(usage_count),
                last_used_at.map(|d| d.to_rfc3339()),
                created_at.to_rfc3339(),
                now.to_rfc3339(),
            ))
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            let source_projects: Vec<String> =
                input.source_project.as_ref().map(|p| vec![p.clone()]).unwrap_or_default();
            let source_observations: Vec<String> =
                input.source_observation.as_ref().map(|o| vec![o.clone()]).unwrap_or_default();

            sqlx::query(&format!(
                "INSERT INTO global_knowledge ({KNOWLEDGE_COLUMNS})
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"
            ))
            .bind(&id)
            .bind(input.knowledge_type.as_str())
            .bind(&input.title)
            .bind(&input.description)
            .bind(&input.instructions)
            .bind(serde_json::to_value(&input.triggers)?)
            .bind(serde_json::to_value(&source_projects)?)
            .bind(serde_json::to_value(&source_observations)?)
            .bind(0.5f64)
            .bind(0i64)
            .bind(Option::<DateTime<Utc>>::None)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;

            Ok(GlobalKnowledge::new(
                id,
                input.knowledge_type,
                input.title,
                input.description,
                input.instructions,
                input.triggers,
                source_projects,
                source_observations,
                0.5,
                0,
                None,
                now.to_rfc3339(),
                now.to_rfc3339(),
            ))
        }
    }

    async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>> {
        let row =
            sqlx::query(&format!("SELECT {KNOWLEDGE_COLUMNS} FROM global_knowledge WHERE id = $1"))
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        row.map(|r| row_to_knowledge(&r)).transpose()
    }

    async fn delete_knowledge(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM global_knowledge WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>> {
        let tsquery = build_tsquery(query);
        if tsquery.is_empty() {
            return self.list_knowledge(None, limit).await.map(|items| {
                items.into_iter().map(|k| KnowledgeSearchResult::new(k, 1.0)).collect()
            });
        }
        let rows = sqlx::query(&format!(
            "SELECT {KNOWLEDGE_COLUMNS},
                    ts_rank_cd(search_vec, to_tsquery('english', $1))::float8 as score
             FROM global_knowledge
             WHERE search_vec @@ to_tsquery('english', $1)
             ORDER BY score DESC
             LIMIT $2"
        ))
        .bind(&tsquery)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                let score: f64 = r.try_get("score")?;
                Ok(KnowledgeSearchResult::new(row_to_knowledge(r)?, score))
            })
            .collect()
    }

    async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>> {
        let rows = if let Some(kt) = knowledge_type {
            sqlx::query(&format!(
                "SELECT {KNOWLEDGE_COLUMNS} FROM global_knowledge
                 WHERE knowledge_type = $1
                 ORDER BY confidence DESC, usage_count DESC LIMIT $2"
            ))
            .bind(kt.as_str())
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(&format!(
                "SELECT {KNOWLEDGE_COLUMNS} FROM global_knowledge
                 ORDER BY confidence DESC, usage_count DESC LIMIT $1"
            ))
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?
        };
        rows.iter().map(row_to_knowledge).collect()
    }

    async fn update_knowledge_usage(&self, id: &str) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            "UPDATE global_knowledge
             SET usage_count = usage_count + 1,
                 last_used_at = $1, updated_at = $1,
                 confidence = LEAST(1.0, confidence + 0.05)
             WHERE id = $2",
        )
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

// ── SummaryStore ─────────────────────────────────────────────────

#[async_trait]
impl SummaryStore for PgStorage {
    async fn save_summary(&self, summary: &SessionSummary) -> Result<()> {
        sqlx::query(&format!(
            "INSERT INTO session_summaries ({SUMMARY_COLUMNS})
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
             ON CONFLICT (session_id) DO UPDATE SET
               project = EXCLUDED.project, request = EXCLUDED.request,
               investigated = EXCLUDED.investigated, learned = EXCLUDED.learned,
               completed = EXCLUDED.completed, next_steps = EXCLUDED.next_steps,
               notes = EXCLUDED.notes, files_read = EXCLUDED.files_read,
               files_edited = EXCLUDED.files_edited, prompt_number = EXCLUDED.prompt_number,
               discovery_tokens = EXCLUDED.discovery_tokens, created_at = EXCLUDED.created_at"
        ))
        .bind(&summary.session_id)
        .bind(&summary.project)
        .bind(&summary.request)
        .bind(&summary.investigated)
        .bind(&summary.learned)
        .bind(&summary.completed)
        .bind(&summary.next_steps)
        .bind(&summary.notes)
        .bind(serde_json::to_value(&summary.files_read)?)
        .bind(serde_json::to_value(&summary.files_edited)?)
        .bind(summary.prompt_number.map(|v| v as i32))
        .bind(summary.discovery_tokens.map(|v| v as i32))
        .bind(summary.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_session_summary(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        let row = sqlx::query(&format!(
            "SELECT {SUMMARY_COLUMNS} FROM session_summaries WHERE session_id = $1"
        ))
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| row_to_summary(&r)).transpose()
    }

    async fn update_session_status_with_summary(
        &self,
        session_id: &str,
        status: SessionStatus,
        summary: Option<&str>,
    ) -> Result<()> {
        let ended_at: Option<DateTime<Utc>> = (status != SessionStatus::Active).then(Utc::now);
        sqlx::query("UPDATE sessions SET status = $1, ended_at = $2 WHERE id = $3")
            .bind(status.as_str())
            .bind(ended_at)
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        if let Some(s) = summary {
            let project: Option<String> =
                sqlx::query_scalar("SELECT project FROM sessions WHERE id = $1")
                    .bind(session_id)
                    .fetch_optional(&self.pool)
                    .await?;

            if let Some(proj) = project {
                let now = Utc::now();
                let empty_json = serde_json::json!([]);
                sqlx::query(&format!(
                    "INSERT INTO session_summaries ({SUMMARY_COLUMNS})
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
                     ON CONFLICT (session_id) DO UPDATE SET
                       learned = EXCLUDED.learned, created_at = EXCLUDED.created_at"
                ))
                .bind(session_id)
                .bind(proj)
                .bind(Option::<String>::None)
                .bind(Option::<String>::None)
                .bind(Some(s))
                .bind(Option::<String>::None)
                .bind(Option::<String>::None)
                .bind(Option::<String>::None)
                .bind(&empty_json)
                .bind(&empty_json)
                .bind(Option::<i32>::None)
                .bind(Option::<i32>::None)
                .bind(now)
                .execute(&self.pool)
                .await?;
            }
        }
        Ok(())
    }

    async fn get_summaries_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<SessionSummary>> {
        let total: i64 = if let Some(p) = project {
            sqlx::query_scalar("SELECT COUNT(*) FROM session_summaries WHERE project = $1")
                .bind(p)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM session_summaries")
                .fetch_one(&self.pool)
                .await?
        };

        let rows = if let Some(p) = project {
            sqlx::query(&format!(
                "SELECT {SUMMARY_COLUMNS} FROM session_summaries
                 WHERE project = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
            ))
            .bind(p)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(&format!(
                "SELECT {SUMMARY_COLUMNS} FROM session_summaries
                 ORDER BY created_at DESC LIMIT $1 OFFSET $2"
            ))
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?
        };

        let items: Vec<SessionSummary> = rows.iter().map(row_to_summary).collect::<Result<_>>()?;
        Ok(PaginatedResult {
            items,
            total: total as u64,
            offset: offset as u64,
            limit: limit as u64,
        })
    }

    async fn search_sessions(&self, query: &str, limit: usize) -> Result<Vec<SessionSummary>> {
        let tsquery = build_tsquery(query);
        if tsquery.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(&format!(
            "SELECT {SUMMARY_COLUMNS} FROM session_summaries
             WHERE search_vec @@ to_tsquery('english', $1)
             ORDER BY ts_rank_cd(search_vec, to_tsquery('english', $1)) DESC
             LIMIT $2"
        ))
        .bind(&tsquery)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_summary).collect()
    }
}

// ---------------------------------------------------------------------------
// PendingQueueStore
// ---------------------------------------------------------------------------

#[async_trait]
impl PendingQueueStore for PgStorage {
    async fn queue_message(
        &self,
        session_id: &str,
        tool_name: Option<&str>,
        tool_input: Option<&str>,
        tool_response: Option<&str>,
        project: Option<&str>,
    ) -> Result<i64> {
        let now = Utc::now().timestamp();
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO pending_messages
               (session_id, status, tool_name, tool_input, tool_response, retry_count, created_at_epoch, project)
               VALUES ($1, 'pending', $2, $3, $4, 0, $5, $6)
               RETURNING id",
        )
        .bind(session_id)
        .bind(tool_name)
        .bind(tool_input)
        .bind(tool_response)
        .bind(now)
        .bind(project)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    async fn claim_pending_messages(
        &self,
        limit: usize,
        visibility_timeout_secs: i64,
    ) -> Result<Vec<PendingMessage>> {
        let now = Utc::now().timestamp();
        let stale_threshold = now - visibility_timeout_secs;
        let rows = sqlx::query(
            "UPDATE pending_messages
               SET status = 'processing', claimed_at_epoch = $1
               WHERE id IN (
                   SELECT id FROM pending_messages
                   WHERE status = 'pending'
                      OR (status = 'processing' AND claimed_at_epoch < $2)
                   ORDER BY created_at_epoch ASC
                   LIMIT $3
                   FOR UPDATE SKIP LOCKED
               )
               RETURNING id, session_id, status, tool_name, tool_input, tool_response,
                         retry_count, created_at_epoch, claimed_at_epoch, completed_at_epoch, project",
        )
        .bind(now)
        .bind(stale_threshold)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_pending_message).collect()
    }

    async fn complete_message(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM pending_messages WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn fail_message(&self, id: i64, increment_retry: bool) -> Result<()> {
        if increment_retry {
            sqlx::query(
                "UPDATE pending_messages
                   SET retry_count = retry_count + 1,
                       status = CASE
                           WHEN retry_count + 1 >= $1 THEN 'failed'
                           ELSE 'pending'
                       END,
                       claimed_at_epoch = NULL
                   WHERE id = $2",
            )
            .bind(max_retry_count())
            .bind(id)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query("UPDATE pending_messages SET status = 'failed' WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    async fn get_pending_count(&self) -> Result<usize> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM pending_messages WHERE status = 'pending'")
                .fetch_one(&self.pool)
                .await?;
        Ok(count as usize)
    }

    async fn release_stale_messages(&self, visibility_timeout_secs: i64) -> Result<usize> {
        let now = Utc::now().timestamp();
        let stale_threshold = now - visibility_timeout_secs;
        let result = sqlx::query(
            "UPDATE pending_messages
               SET status = 'pending', claimed_at_epoch = NULL
               WHERE status = 'processing' AND claimed_at_epoch <= $1",
        )
        .bind(stale_threshold)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as usize)
    }

    async fn get_failed_messages(&self, limit: usize) -> Result<Vec<PendingMessage>> {
        let rows = sqlx::query(
            "SELECT id, session_id, status, tool_name, tool_input, tool_response,
                    retry_count, created_at_epoch, claimed_at_epoch, completed_at_epoch, project
               FROM pending_messages
               WHERE status = 'failed'
               ORDER BY created_at_epoch DESC
               LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_pending_message).collect()
    }

    async fn get_all_pending_messages(&self, limit: usize) -> Result<Vec<PendingMessage>> {
        let rows = sqlx::query(
            "SELECT id, session_id, status, tool_name, tool_input, tool_response,
                    retry_count, created_at_epoch, claimed_at_epoch, completed_at_epoch, project
               FROM pending_messages
               ORDER BY created_at_epoch DESC
               LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_pending_message).collect()
    }

    async fn get_queue_stats(&self) -> Result<QueueStats> {
        let row = sqlx::query(
            "SELECT
               COALESCE(SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END), 0) as pending,
               COALESCE(SUM(CASE WHEN status = 'processing' THEN 1 ELSE 0 END), 0) as processing,
               COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) as failed,
               COALESCE(SUM(CASE WHEN status = 'processed' THEN 1 ELSE 0 END), 0) as processed
             FROM pending_messages",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(QueueStats {
            pending: row.try_get::<i64, _>("pending")? as u64,
            processing: row.try_get::<i64, _>("processing")? as u64,
            failed: row.try_get::<i64, _>("failed")? as u64,
            processed: row.try_get::<i64, _>("processed")? as u64,
        })
    }

    async fn clear_failed_messages(&self) -> Result<usize> {
        let result = sqlx::query("DELETE FROM pending_messages WHERE status = 'failed'")
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() as usize)
    }

    async fn retry_failed_messages(&self) -> Result<usize> {
        let result = sqlx::query(
            "UPDATE pending_messages
               SET status = 'pending', retry_count = 0, claimed_at_epoch = NULL
               WHERE status = 'failed'",
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() as usize)
    }

    async fn clear_all_pending_messages(&self) -> Result<usize> {
        let result = sqlx::query("DELETE FROM pending_messages").execute(&self.pool).await?;
        Ok(result.rows_affected() as usize)
    }
}

// ---------------------------------------------------------------------------
// PromptStore
// ---------------------------------------------------------------------------

#[async_trait]
impl PromptStore for PgStorage {
    async fn save_user_prompt(&self, prompt: &UserPrompt) -> Result<()> {
        sqlx::query(
            "INSERT INTO user_prompts (id, content_session_id, prompt_number, prompt_text, project, created_at)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT (id) DO UPDATE SET
                 prompt_text = EXCLUDED.prompt_text, project = EXCLUDED.project",
        )
        .bind(&prompt.id)
        .bind(&prompt.content_session_id)
        .bind(prompt.prompt_number as i32)
        .bind(&prompt.prompt_text)
        .bind(&prompt.project)
        .bind(prompt.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>> {
        let total: i64 = if let Some(p) = project {
            sqlx::query_scalar("SELECT COUNT(*) FROM user_prompts WHERE project = $1")
                .bind(p)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM user_prompts").fetch_one(&self.pool).await?
        };

        let rows = if let Some(p) = project {
            sqlx::query(
                "SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
                   FROM user_prompts WHERE project = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
            )
            .bind(p)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
                   FROM user_prompts ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            )
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?
        };

        let items: Vec<UserPrompt> = rows.iter().map(row_to_prompt).collect::<Result<_>>()?;
        Ok(PaginatedResult {
            items,
            total: total as u64,
            offset: offset as u64,
            limit: limit as u64,
        })
    }

    async fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>> {
        let row = sqlx::query(
            "SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
               FROM user_prompts WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_prompt).transpose()
    }

    async fn search_prompts(&self, query: &str, limit: usize) -> Result<Vec<UserPrompt>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let pattern = format!("%{}%", escape_like(query));
        let rows = sqlx::query(
            "SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
               FROM user_prompts
               WHERE prompt_text ILIKE $1
               ORDER BY created_at DESC
               LIMIT $2",
        )
        .bind(&pattern)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_prompt).collect()
    }
}

// ---------------------------------------------------------------------------
// StatsStore
// ---------------------------------------------------------------------------

#[async_trait]
impl StatsStore for PgStorage {
    async fn get_stats(&self) -> Result<StorageStats> {
        let observation_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM observations").fetch_one(&self.pool).await?;
        let session_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sessions").fetch_one(&self.pool).await?;
        let summary_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_summaries")
            .fetch_one(&self.pool)
            .await?;
        let prompt_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM user_prompts").fetch_one(&self.pool).await?;
        let project_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT project) FROM observations WHERE project IS NOT NULL",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(StorageStats {
            observation_count: observation_count as u64,
            session_count: session_count as u64,
            summary_count: summary_count as u64,
            prompt_count: prompt_count as u64,
            project_count: project_count as u64,
        })
    }

    async fn get_all_projects(&self) -> Result<Vec<String>> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT project FROM observations WHERE project IS NOT NULL ORDER BY project",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn get_observations_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<Observation>> {
        let total: i64 = if let Some(p) = project {
            sqlx::query_scalar("SELECT COUNT(*) FROM observations WHERE project = $1")
                .bind(p)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM observations").fetch_one(&self.pool).await?
        };

        let rows = if let Some(p) = project {
            sqlx::query(
                "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                        facts, concepts, files_read, files_modified, keywords, prompt_number,
                        discovery_tokens, noise_level, noise_reason, created_at
                   FROM observations WHERE project = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
            )
            .bind(p)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                        facts, concepts, files_read, files_modified, keywords, prompt_number,
                        discovery_tokens, noise_level, noise_reason, created_at
                   FROM observations ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            )
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&self.pool)
            .await?
        };
        let items: Vec<Observation> = rows.iter().map(row_to_observation).collect::<Result<_>>()?;
        Ok(PaginatedResult {
            items,
            total: total as u64,
            offset: offset as u64,
            limit: limit as u64,
        })
    }
}

// ---------------------------------------------------------------------------
// SearchStore
// ---------------------------------------------------------------------------

#[async_trait]
impl SearchStore for PgStorage {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let tsquery = build_tsquery(query);
        if tsquery.is_empty() {
            return self.get_recent(limit).await;
        }
        let rows = sqlx::query(
            "SELECT id, title, subtitle, observation_type, noise_level,
                    ts_rank_cd(search_vec, to_tsquery('english', $1))::float8 as score
               FROM observations
               WHERE search_vec @@ to_tsquery('english', $1)
               ORDER BY score DESC
               LIMIT $2",
        )
        .bind(&tsquery)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_search_result).collect()
    }

    async fn hybrid_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let keywords: HashSet<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let tsquery = build_tsquery(query);
        if tsquery.is_empty() {
            return self.get_recent(limit).await;
        }

        let rows = sqlx::query(
            "SELECT id, title, subtitle, observation_type, noise_level, keywords,
                    ts_rank_cd(search_vec, to_tsquery('english', $1))::float8 as fts_score
               FROM observations
               WHERE search_vec @@ to_tsquery('english', $1)
               ORDER BY fts_score DESC
               LIMIT $2",
        )
        .bind(&tsquery)
        .bind((limit * 2) as i64)
        .fetch_all(&self.pool)
        .await?;

        let raw_results: Vec<(SearchResult, f64, HashSet<String>)> = rows
            .iter()
            .map(|row| {
                let obs_type_str: String = row.try_get("observation_type")?;
                let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
                    .unwrap_or_else(|_| obs_type_str.parse().unwrap_or(ObservationType::Change));
                let noise_str: Option<String> = row.try_get("noise_level")?;
                let noise_level =
                    noise_str.and_then(|s| s.parse::<NoiseLevel>().ok()).unwrap_or_default();
                let fts_score: f64 = row.try_get("fts_score")?;
                let kw_json: serde_json::Value = row.try_get("keywords")?;
                let obs_kw: HashSet<String> = serde_json::from_value::<Vec<String>>(kw_json)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|s| s.to_lowercase())
                    .collect();
                let sr = SearchResult::new(
                    row.try_get("id")?,
                    row.try_get("title")?,
                    row.try_get("subtitle")?,
                    obs_type,
                    noise_level,
                    0.0,
                );
                Ok((sr, fts_score, obs_kw))
            })
            .collect::<Result<_>>()?;

        let (min_fts, max_fts) =
            raw_results.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(mn, mx), (_, fts, _)| {
                (mn.min(*fts), mx.max(*fts))
            });
        let fts_range = max_fts - min_fts;

        let mut results: Vec<(SearchResult, f64)> = raw_results
            .into_iter()
            .map(|(mut result, fts_score, obs_kw)| {
                let fts_normalized =
                    if fts_range > 0.0 { (fts_score - min_fts) / fts_range } else { 1.0 };
                let keyword_overlap = keywords.intersection(&obs_kw).count() as f64;
                let keyword_score =
                    if keywords.is_empty() { 0.0 } else { keyword_overlap / keywords.len() as f64 };
                result.score = fts_normalized.mul_add(0.7, keyword_score * 0.3);
                let score = result.score;
                (result, score)
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        Ok(results.into_iter().take(limit).map(|(r, _)| r).collect())
    }

    async fn search_with_filters(
        &self,
        query: Option<&str>,
        project: Option<&str>,
        obs_type: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let mut conditions = Vec::new();
        let mut param_idx: usize = 1;
        let mut bind_strings: Vec<String> = Vec::new();

        if let Some(p) = project {
            conditions.push(format!("project = ${param_idx}"));
            param_idx += 1;
            bind_strings.push(p.to_owned());
        }
        if let Some(t) = obs_type {
            conditions.push(format!("observation_type = ${param_idx}"));
            param_idx += 1;
            bind_strings.push(serde_json::to_string(t).unwrap_or_else(|_| format!("\"{t}\"")));
        }
        if let Some(f) = from {
            conditions.push(format!("created_at >= ${param_idx}::timestamptz"));
            param_idx += 1;
            bind_strings.push(f.to_owned());
        }
        if let Some(t) = to {
            conditions.push(format!("created_at <= ${param_idx}::timestamptz"));
            param_idx += 1;
            bind_strings.push(t.to_owned());
        }

        if let Some(q) = query {
            let tsquery = build_tsquery(q);
            if !tsquery.is_empty() {
                let fts_cond = format!("search_vec @@ to_tsquery('english', ${param_idx})");
                param_idx += 1;
                let score_expr = format!(
                    "ts_rank_cd(search_vec, to_tsquery('english', ${}))::float8 as score",
                    param_idx - 1
                );
                let extra_where = if conditions.is_empty() {
                    String::new()
                } else {
                    format!("AND {}", conditions.join(" AND "))
                };
                let sql = format!(
                    "SELECT id, title, subtitle, observation_type, noise_level, {score_expr}
                       FROM observations
                       WHERE {fts_cond} {extra_where}
                       ORDER BY score DESC
                       LIMIT ${param_idx}"
                );

                let mut q = sqlx::query(&sql);
                for val in &bind_strings {
                    q = q.bind(val);
                }
                q = q.bind(&tsquery);
                q = q.bind(limit as i64);
                let rows = q.fetch_all(&self.pool).await?;
                return rows.iter().map(row_to_search_result).collect();
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let sql = format!(
            "SELECT id, title, subtitle, observation_type, noise_level
               FROM observations {where_clause}
               ORDER BY created_at DESC
               LIMIT ${param_idx}"
        );

        let mut q = sqlx::query(&sql);
        for val in &bind_strings {
            q = q.bind(val);
        }
        q = q.bind(limit as i64);
        let rows = q.fetch_all(&self.pool).await?;
        rows.iter().map(row_to_search_result_default).collect()
    }

    async fn get_timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let mut conditions = Vec::new();
        let mut param_idx: usize = 1;
        let mut bind_strings: Vec<String> = Vec::new();

        if let Some(f) = from {
            conditions.push(format!("created_at >= ${param_idx}::timestamptz"));
            param_idx += 1;
            bind_strings.push(f.to_owned());
        }
        if let Some(t) = to {
            conditions.push(format!("created_at <= ${param_idx}::timestamptz"));
            param_idx += 1;
            bind_strings.push(t.to_owned());
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };
        let sql = format!(
            "SELECT id, title, subtitle, observation_type, noise_level
               FROM observations {where_clause}
               ORDER BY created_at DESC
               LIMIT ${param_idx}"
        );

        let mut q = sqlx::query(&sql);
        for val in &bind_strings {
            q = q.bind(val);
        }
        q = q.bind(limit as i64);
        let rows = q.fetch_all(&self.pool).await?;
        rows.iter().map(row_to_search_result_default).collect()
    }

    async fn semantic_search(&self, query_vec: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        if query_vec.is_empty() {
            return Ok(Vec::new());
        }

        let has_embeddings: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM observations WHERE embedding IS NOT NULL")
                .fetch_one(&self.pool)
                .await?;
        if has_embeddings == 0 {
            return Ok(Vec::new());
        }

        let vec_str =
            format!("[{}]", query_vec.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));
        let rows = sqlx::query(
            "SELECT id, title, subtitle, observation_type, noise_level,
                    1.0 - (embedding <=> $1::vector) as score
               FROM observations
               WHERE embedding IS NOT NULL
               ORDER BY embedding <=> $1::vector
               LIMIT $2",
        )
        .bind(&vec_str)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_search_result).collect()
    }

    #[expect(
        clippy::too_many_lines,
        reason = "hybrid v2 combines FTS + vector scoring in two passes"
    )]
    async fn hybrid_search_v2(
        &self,
        query: &str,
        query_vec: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        if query_vec.is_empty() {
            return self.hybrid_search(query, limit).await;
        }

        let has_embeddings: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM observations WHERE embedding IS NOT NULL")
                .fetch_one(&self.pool)
                .await?;
        if has_embeddings == 0 {
            return self.hybrid_search(query, limit).await;
        }

        let tsquery = build_tsquery(query);
        let mut fts_scores: HashMap<String, f64> = HashMap::new();
        let mut max_fts_score: f64 = 0.0;

        if !tsquery.is_empty() {
            let fts_rows = sqlx::query(
                "SELECT id, ts_rank_cd(search_vec, to_tsquery('english', $1))::float8 as fts_score
                   FROM observations
                   WHERE search_vec @@ to_tsquery('english', $1)
                   LIMIT $2",
            )
            .bind(&tsquery)
            .bind((limit * 3) as i64)
            .fetch_all(&self.pool)
            .await?;

            for row in &fts_rows {
                let id: String = row.try_get("id")?;
                let score: f64 = row.try_get("fts_score")?;
                if score > max_fts_score {
                    max_fts_score = score;
                }
                fts_scores.insert(id, score);
            }
        }

        let vec_str =
            format!("[{}]", query_vec.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));
        let vec_rows = sqlx::query(
            "SELECT id, 1.0 - (embedding <=> $1::vector) as similarity
               FROM observations
               WHERE embedding IS NOT NULL
               ORDER BY embedding <=> $1::vector
               LIMIT $2",
        )
        .bind(&vec_str)
        .bind((limit * 3) as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut vec_scores: HashMap<String, f64> = HashMap::new();
        for row in &vec_rows {
            let id: String = row.try_get("id")?;
            let sim: f64 = row.try_get("similarity")?;
            vec_scores.insert(id, sim);
        }

        let all_ids: HashSet<String> =
            fts_scores.keys().chain(vec_scores.keys()).cloned().collect();

        let mut combined: Vec<(String, f64)> = all_ids
            .into_iter()
            .map(|id| {
                let fts_normalized = if max_fts_score > 0.0_f64 {
                    fts_scores.get(&id).copied().unwrap_or(0.0_f64) / max_fts_score
                } else {
                    0.0_f64
                };
                let vec_sim = vec_scores.get(&id).copied().unwrap_or(0.0_f64);
                let final_score = fts_normalized.mul_add(0.5_f64, vec_sim * 0.5_f64);
                (id, final_score)
            })
            .collect();

        combined.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

        let top_ids: Vec<String> = combined.into_iter().take(limit).map(|(id, _)| id).collect();
        if top_ids.is_empty() {
            return Ok(Vec::new());
        }

        let score_lookup: HashMap<String, f64> = fts_scores
            .keys()
            .chain(vec_scores.keys())
            .map(|id| {
                let fts_normalized = if max_fts_score > 0.0_f64 {
                    fts_scores.get(id).copied().unwrap_or(0.0_f64) / max_fts_score
                } else {
                    0.0_f64
                };
                let vec_sim = vec_scores.get(id).copied().unwrap_or(0.0_f64);
                (id.clone(), fts_normalized.mul_add(0.5_f64, vec_sim * 0.5_f64))
            })
            .collect();

        let placeholders: String =
            (1..=top_ids.len()).map(|i| format!("${i}")).collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, title, subtitle, observation_type, noise_level
               FROM observations WHERE id IN ({placeholders})"
        );

        let mut q = sqlx::query(&sql);
        for id in &top_ids {
            q = q.bind(id);
        }
        let rows = q.fetch_all(&self.pool).await?;

        let mut results: Vec<SearchResult> = rows
            .iter()
            .map(|row| {
                let obs_type_str: String = row.try_get("observation_type")?;
                let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
                    .unwrap_or_else(|_| obs_type_str.parse().unwrap_or(ObservationType::Change));
                let noise_str: Option<String> = row.try_get("noise_level")?;
                let noise_level =
                    noise_str.and_then(|s| s.parse::<NoiseLevel>().ok()).unwrap_or_default();
                let id: String = row.try_get("id")?;
                let score = score_lookup.get(&id).copied().unwrap_or(0.0_f64);
                Ok(SearchResult::new(
                    id,
                    row.try_get("title")?,
                    row.try_get("subtitle")?,
                    obs_type,
                    noise_level,
                    score,
                ))
            })
            .collect::<Result<_>>()?;

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
        Ok(results)
    }
}

// ---------------------------------------------------------------------------
// EmbeddingStore
// ---------------------------------------------------------------------------

#[async_trait]
impl EmbeddingStore for PgStorage {
    async fn store_embedding(&self, observation_id: &str, embedding: &[f32]) -> Result<()> {
        let vec_str =
            format!("[{}]", embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));
        sqlx::query("UPDATE observations SET embedding = $1::vector WHERE id = $2")
            .bind(&vec_str)
            .bind(observation_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_observations_without_embeddings(&self, limit: usize) -> Result<Vec<Observation>> {
        let rows = sqlx::query(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative,
                    facts, concepts, files_read, files_modified, keywords, prompt_number,
                    discovery_tokens, noise_level, noise_reason, created_at
               FROM observations
               WHERE embedding IS NULL
               LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_observation).collect()
    }

    async fn clear_embeddings(&self) -> Result<()> {
        sqlx::query("UPDATE observations SET embedding = NULL").execute(&self.pool).await?;
        Ok(())
    }
}
