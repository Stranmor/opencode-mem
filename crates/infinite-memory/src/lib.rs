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

/// Configuration for InfiniteMemory
#[derive(Debug, Clone)]
pub struct InfiniteMemoryConfig {
    pub api_base: String,
    pub model: String,
}

impl Default for InfiniteMemoryConfig {
    fn default() -> Self {
        Self {
            api_base: "https://antigravity.quantumind.ru".to_string(),
            model: "gemini-3-flash".to_string(),
        }
    }
}

/// Infinite memory storage client
pub struct InfiniteMemory {
    pool: PgPool,
    api_key: String,
    config: InfiniteMemoryConfig,
    http_client: reqwest::Client,
}

impl InfiniteMemory {
    /// Create new connection with default config
    pub async fn new(database_url: &str, api_key: &str) -> Result<Self> {
        Self::with_config(database_url, api_key, InfiniteMemoryConfig::default()).await
    }

    /// Create new connection with custom config
    pub async fn with_config(
        database_url: &str,
        api_key: &str,
        config: InfiniteMemoryConfig,
    ) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        Ok(Self {
            pool,
            api_key: api_key.to_string(),
            config,
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
    /// Truncates large content to prevent context window exhaustion
    pub async fn compress_events(&self, events: &[StoredEvent]) -> Result<String> {
        const MAX_CONTENT_CHARS: usize = 500;
        const MAX_TOTAL_CHARS: usize = 8000;
        
        if events.is_empty() {
            return Ok(String::new());
        }

        let mut events_text: Vec<String> = Vec::with_capacity(events.len());
        let mut total_chars = 0usize;
        
        for e in events {
            let content_str = serde_json::to_string(&e.content).unwrap_or_default();
            let truncated = if content_str.len() > MAX_CONTENT_CHARS {
                format!("{}...(truncated)", &content_str[..MAX_CONTENT_CHARS])
            } else {
                content_str
            };
            let line = format!(
                "[{}] {}: {}",
                e.event_type,
                e.ts.format("%H:%M:%S"),
                truncated
            );
            total_chars += line.len();
            if total_chars > MAX_TOTAL_CHARS {
                events_text.push(format!("...({} more events truncated)", events.len() - events_text.len()));
                break;
            }
            events_text.push(line);
        }

        let prompt = format!(
            "Сожми эти {} событий в 2-3 ключевых факта на русском. \
             Сохрани: файлы, решения, ошибки, результаты. Без воды.\n\n{}",
            events.len(),
            events_text.join("\n")
        );

        let response = self.http_client
            .post(format!("{}/v1/chat/completions", self.config.api_base))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&serde_json::json!({
                "model": self.config.model,
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
    /// Groups events by session_id to prevent context leakage between sessions
    pub async fn run_compression_pipeline(&self) -> Result<u32> {
        let events = self.get_unsummarized_events(100).await?;
        if events.is_empty() {
            return Ok(0);
        }

        // Group events by session_id
        let mut sessions: std::collections::HashMap<String, Vec<StoredEvent>> =
            std::collections::HashMap::new();
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
            let summary = self.compress_events(&session_events).await?;
            self.create_5min_summary(&session_events, &summary).await?;
            total_processed += session_events.len() as u32;
        }

        Ok(total_processed)
    }

    /// Get 5-min summaries not yet aggregated into hour summaries
    pub async fn get_unaggregated_5min_summaries(&self, limit: i64) -> Result<Vec<Summary>> {
        let rows = sqlx::query_as::<_, (i64, DateTime<Utc>, DateTime<Utc>, Option<String>, String, i32)>(
            r#"
            SELECT id, ts_start, ts_end, project, content, event_count
            FROM summaries_5min
            WHERE summary_hour_id IS NULL
            ORDER BY ts_start ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, ts_start, ts_end, project, content, event_count)| Summary {
                id,
                ts_start,
                ts_end,
                project,
                content,
                event_count,
            })
            .collect())
    }

    /// Create hour summary from 5-min summaries
    pub async fn create_hour_summary(&self, summaries: &[Summary], content: &str) -> Result<i64> {
        if summaries.is_empty() {
            return Ok(0);
        }

        let ts_start = summaries.first().expect("BUG: empty summaries after check").ts_start;
        let ts_end = summaries.last().expect("BUG: empty summaries after check").ts_end;
        let project = summaries.first().and_then(|s| s.project.clone());
        let total_events: i32 = summaries.iter().map(|s| s.event_count).sum();

        let mut tx = self.pool.begin().await?;

        let row: (i64,) = sqlx::query_as(
            r#"
            INSERT INTO summaries_hour (ts_start, ts_end, project, content, event_count)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#,
        )
        .bind(ts_start)
        .bind(ts_end)
        .bind(&project)
        .bind(content)
        .bind(total_events)
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

    /// Get hour summaries not yet aggregated into day summaries
    pub async fn get_unaggregated_hour_summaries(&self, limit: i64) -> Result<Vec<Summary>> {
        let rows = sqlx::query_as::<_, (i64, DateTime<Utc>, DateTime<Utc>, Option<String>, String, i32)>(
            r#"
            SELECT id, ts_start, ts_end, project, content, event_count
            FROM summaries_hour
            WHERE summary_day_id IS NULL
            ORDER BY ts_start ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, ts_start, ts_end, project, content, event_count)| Summary {
                id,
                ts_start,
                ts_end,
                project,
                content,
                event_count,
            })
            .collect())
    }

    /// Create day summary from hour summaries
    pub async fn create_day_summary(&self, summaries: &[Summary], content: &str) -> Result<i64> {
        if summaries.is_empty() {
            return Ok(0);
        }

        let ts_start = summaries.first().expect("BUG: empty summaries after check").ts_start;
        let ts_end = summaries.last().expect("BUG: empty summaries after check").ts_end;
        let project = summaries.first().and_then(|s| s.project.clone());
        let total_events: i32 = summaries.iter().map(|s| s.event_count).sum();

        let mut tx = self.pool.begin().await?;

        let row: (i64,) = sqlx::query_as(
            r#"
            INSERT INTO summaries_day (ts_start, ts_end, project, content, event_count)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#,
        )
        .bind(ts_start)
        .bind(ts_end)
        .bind(&project)
        .bind(content)
        .bind(total_events)
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

    /// Compress summaries into higher-level summary
    pub async fn compress_summaries(&self, summaries: &[Summary]) -> Result<String> {
        if summaries.is_empty() {
            return Ok(String::new());
        }

        let summaries_text: Vec<String> = summaries
            .iter()
            .map(|s| format!("[{} - {}] {}", s.ts_start.format("%H:%M"), s.ts_end.format("%H:%M"), s.content))
            .collect();

        let prompt = format!(
            "Объедини эти {} сводок в одну краткую сводку на русском (2-3 предложения). \
             Сохрани ключевые факты, файлы, решения.\n\n{}",
            summaries.len(),
            summaries_text.join("\n\n")
        );

        let response = self.http_client
            .post(format!("{}/v1/chat/completions", self.config.api_base))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&serde_json::json!({
                "model": self.config.model,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": 300
            }))
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;

        Ok(response["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string())
    }

    /// Run full hierarchical compression pipeline
    pub async fn run_full_compression(&self) -> Result<(u32, u32, u32)> {
        let events_processed = self.run_compression_pipeline().await?;
        
        let summaries_5min = self.get_unaggregated_5min_summaries(12).await?;
        let hours_created = if summaries_5min.len() >= 6 {
            let content = self.compress_summaries(&summaries_5min).await?;
            self.create_hour_summary(&summaries_5min, &content).await?;
            1u32
        } else {
            0u32
        };

        let summaries_hour = self.get_unaggregated_hour_summaries(24).await?;
        let days_created = if summaries_hour.len() >= 12 {
            let content = self.compress_summaries(&summaries_hour).await?;
            self.create_day_summary(&summaries_hour, &content).await?;
            1u32
        } else {
            0u32
        };

        Ok((events_processed, hours_created, days_created))
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
