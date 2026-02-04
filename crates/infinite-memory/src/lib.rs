//! Infinite AGI Memory - PostgreSQL + pgvector storage
//!
//! Stores all events (user, assistant, tool, decision, error, commit, delegation)
//! with hierarchical summarization using Gemini Flash.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Event types that can be stored
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventType {
    User,
    Assistant,
    Tool,
    Decision,
    Error,
    Commit,
    Delegation,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
            Self::Decision => "decision",
            Self::Error => "error",
            Self::Commit => "commit",
            Self::Delegation => "delegation",
        }
    }
}

/// Raw event to be stored
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawEvent {
    pub session_id: String,
    pub project: Option<String>,
    pub event_type: EventType,
    pub content: serde_json::Value,
    pub files: Vec<String>,
    pub tools: Vec<String>,
}

/// Stored event with ID and timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub id: i64,
    pub ts: DateTime<Utc>,
    pub session_id: String,
    pub project: Option<String>,
    pub event_type: String,
    pub content: serde_json::Value,
    pub files: Vec<String>,
    pub tools: Vec<String>,
}

/// Summary at various time scales
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub id: i64,
    pub ts_start: DateTime<Utc>,
    pub ts_end: DateTime<Utc>,
    pub project: Option<String>,
    pub content: String,
    pub event_count: i32,
}

/// Infinite memory storage client
pub struct InfiniteMemory {
    pool: PgPool,
    api_key: String,
    api_base: String,
    http_client: reqwest::Client,
}

impl InfiniteMemory {
    /// Create new connection to infinite memory
    pub async fn new(database_url: &str, api_key: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        Ok(Self {
            pool,
            api_key: api_key.to_string(),
            api_base: "https://antigravity.quantumind.ru".to_string(),
            http_client: reqwest::Client::new(),
        })
    }

    /// Store a raw event
    pub async fn store_event(&self, event: RawEvent) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            r#"
            INSERT INTO raw_events (session_id, project, event_type, content, files, tools)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
        )
        .bind(&event.session_id)
        .bind(&event.project)
        .bind(event.event_type.as_str())
        .bind(&event.content)
        .bind(&event.files)
        .bind(&event.tools)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }

    /// Get recent events
    pub async fn get_recent(&self, limit: i64) -> Result<Vec<StoredEvent>> {
        let rows = sqlx::query_as::<_, (i64, DateTime<Utc>, String, Option<String>, String, serde_json::Value, Vec<String>, Vec<String>)>(
            r#"
            SELECT id, ts, session_id, project, event_type, content, files, tools
            FROM raw_events
            ORDER BY ts DESC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, ts, session_id, project, event_type, content, files, tools)| StoredEvent {
                id,
                ts,
                session_id,
                project,
                event_type,
                content,
                files,
                tools,
            })
            .collect())
    }

    /// Get events without 5-min summary (for compression pipeline)
    pub async fn get_unsummarized_events(&self, limit: i64) -> Result<Vec<StoredEvent>> {
        let rows = sqlx::query_as::<_, (i64, DateTime<Utc>, String, Option<String>, String, serde_json::Value, Vec<String>, Vec<String>)>(
            r#"
            SELECT id, ts, session_id, project, event_type, content, files, tools
            FROM raw_events
            WHERE summary_5min_id IS NULL
            ORDER BY ts ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, ts, session_id, project, event_type, content, files, tools)| StoredEvent {
                id,
                ts,
                session_id,
                project,
                event_type,
                content,
                files,
                tools,
            })
            .collect())
    }

    /// Compress events using Gemini Flash
    pub async fn compress_events(&self, events: &[StoredEvent]) -> Result<String> {
        if events.is_empty() {
            return Ok(String::new());
        }

        let events_text: Vec<String> = events
            .iter()
            .map(|e| {
                format!(
                    "[{}] {}: {}",
                    e.event_type,
                    e.ts.format("%H:%M:%S"),
                    serde_json::to_string(&e.content).unwrap_or_default()
                )
            })
            .collect();

        let prompt = format!(
            "Сожми эти {} событий в 2-3 ключевых факта на русском. \
             Сохрани: файлы, решения, ошибки, результаты. Без воды.\n\n{}",
            events.len(),
            events_text.join("\n")
        );

        let response = self.http_client
            .post(format!("{}/v1/chat/completions", self.api_base))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&serde_json::json!({
                "model": "gemini-3-flash",
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": 500
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;

        let content = response["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(content)
    }

    /// Create 5-minute summary and link events
    pub async fn create_5min_summary(&self, events: &[StoredEvent], summary: &str) -> Result<i64> {
        if events.is_empty() {
            return Ok(0);
        }

        let ts_start = events.first().expect("BUG: create_5min_summary called with empty events after is_empty check").ts;
        let ts_end = events.last().expect("BUG: create_5min_summary called with empty events after is_empty check").ts;
        let session_id = events.first().map(|e| e.session_id.clone());
        let project = events.first().and_then(|e| e.project.clone());

        let mut tx = self.pool.begin().await?;

        let row: (i64,) = sqlx::query_as(
            r#"
            INSERT INTO summaries_5min (ts_start, ts_end, session_id, project, content, event_count)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
        )
        .bind(ts_start)
        .bind(ts_end)
        .bind(&session_id)
        .bind(&project)
        .bind(summary)
        .bind(events.len() as i32)
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

    /// Run compression pipeline (call periodically)
    pub async fn run_compression_pipeline(&self) -> Result<u32> {
        let events = self.get_unsummarized_events(50).await?;
        if events.is_empty() {
            return Ok(0);
        }

        tracing::info!("Compressing {} events", events.len());
        let summary = self.compress_events(&events).await?;
        self.create_5min_summary(&events, &summary).await?;

        Ok(events.len() as u32)
    }

    /// Search events by text (FTS on content)
    pub async fn search(&self, query: &str, limit: i64) -> Result<Vec<StoredEvent>> {
        let rows = sqlx::query_as::<_, (i64, DateTime<Utc>, String, Option<String>, String, serde_json::Value, Vec<String>, Vec<String>)>(
            r#"
            SELECT id, ts, session_id, project, event_type, content, files, tools
            FROM raw_events
            WHERE content::text ILIKE '%' || $1 || '%'
            ORDER BY ts DESC
            LIMIT $2
            "#,
        )
        .bind(query)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, ts, session_id, project, event_type, content, files, tools)| StoredEvent {
                id,
                ts,
                session_id,
                project,
                event_type,
                content,
                files,
                tools,
            })
            .collect())
    }

    /// Get statistics
    pub async fn stats(&self) -> Result<serde_json::Value> {
        let event_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM raw_events")
            .fetch_one(&self.pool)
            .await?;

        let summary_5min_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM summaries_5min")
            .fetch_one(&self.pool)
            .await?;

        let summary_hour_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM summaries_hour")
            .fetch_one(&self.pool)
            .await?;

        let summary_day_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM summaries_day")
            .fetch_one(&self.pool)
            .await?;

        Ok(serde_json::json!({
            "raw_events": event_count.0,
            "summaries_5min": summary_5min_count.0,
            "summaries_hour": summary_hour_count.0,
            "summaries_day": summary_day_count.0
        }))
    }
}

/// Helper to create tool event
pub fn tool_event(
    session_id: &str,
    project: Option<&str>,
    tool: &str,
    input: serde_json::Value,
    output: serde_json::Value,
    files: Vec<String>,
) -> RawEvent {
    RawEvent {
        session_id: session_id.to_string(),
        project: project.map(|s| s.to_string()),
        event_type: EventType::Tool,
        content: serde_json::json!({
            "tool": tool,
            "input": input,
            "output": output
        }),
        files,
        tools: vec![tool.to_string()],
    }
}

/// Helper to create user message event
pub fn user_event(session_id: &str, project: Option<&str>, message: &str) -> RawEvent {
    RawEvent {
        session_id: session_id.to_string(),
        project: project.map(|s| s.to_string()),
        event_type: EventType::User,
        content: serde_json::json!({
            "text": message
        }),
        files: vec![],
        tools: vec![],
    }
}

/// Helper to create assistant response event
pub fn assistant_event(
    session_id: &str,
    project: Option<&str>,
    response: &str,
    thinking: Option<&str>,
) -> RawEvent {
    RawEvent {
        session_id: session_id.to_string(),
        project: project.map(|s| s.to_string()),
        event_type: EventType::Assistant,
        content: serde_json::json!({
            "text": response,
            "thinking": thinking
        }),
        files: vec![],
        tools: vec![],
    }
}
