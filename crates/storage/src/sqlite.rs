//! SQLite storage implementation

use anyhow::Result;
use chrono::Utc;
use opencode_mem_core::{
    Observation, ObservationType, SearchResult, Session, SessionStatus, SessionSummary, UserPrompt,
};
use rusqlite::{params, Connection};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use crate::migrations;

pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

fn lock_conn<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|e: PoisonError<_>| anyhow::anyhow!("Database lock poisoned: {}", e))
}

fn log_row_error<T>(result: rusqlite::Result<T>) -> Option<T> {
    match result {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("Row read error: {}", e);
            None
        }
    }
}

impl Storage {
    pub fn new(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        let conn = lock_conn(&storage.conn)?;
        migrations::run_migrations(&conn)?;
        drop(conn);

        Ok(storage)
    }

    pub fn save_session(&self, session: &Session) -> Result<()> {
        let conn = lock_conn(&self.conn)?;
        conn.execute(
            r#"INSERT OR REPLACE INTO sessions 
               (id, content_session_id, memory_session_id, project, user_prompt, started_at, ended_at, status, prompt_counter)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
            params![
                session.id,
                session.content_session_id,
                session.memory_session_id,
                session.project,
                session.user_prompt,
                session.started_at.with_timezone(&Utc).to_rfc3339(),
                session.ended_at.map(|d| d.with_timezone(&Utc).to_rfc3339()),
                serde_json::to_string(&session.status)?,
                session.prompt_counter,
            ],
        )?;
        Ok(())
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let conn = lock_conn(&self.conn)?;
        let mut stmt = conn.prepare(
            r#"SELECT id, content_session_id, memory_session_id, project, user_prompt, started_at, ended_at, status, prompt_counter
               FROM sessions WHERE id = ?1"#,
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            let started_at_str: String = row.get(5)?;
            let started_at = chrono::DateTime::parse_from_rfc3339(&started_at_str)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                .with_timezone(&Utc);

            let ended_at_str: Option<String> = row.get(6)?;
            let ended_at = ended_at_str
                .map(|s| chrono::DateTime::parse_from_rfc3339(&s))
                .transpose()
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                .map(|d| d.with_timezone(&Utc));

            let status_str: String = row.get(7)?;
            let status = serde_json::from_str(&status_str)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            Ok(Some(Session {
                id: row.get(0)?,
                content_session_id: row.get(1)?,
                memory_session_id: row.get(2)?,
                project: row.get(3)?,
                user_prompt: row.get(4)?,
                started_at,
                ended_at,
                status,
                prompt_counter: row.get(8)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn update_session_status(&self, id: &str, status: SessionStatus) -> Result<()> {
        let conn = lock_conn(&self.conn)?;
        conn.execute(
            "UPDATE sessions SET status = ?1, ended_at = ?2 WHERE id = ?3",
            params![
                serde_json::to_string(&status)?,
                if status != SessionStatus::Active {
                    Some(Utc::now().to_rfc3339())
                } else {
                    None
                },
                id
            ],
        )?;
        Ok(())
    }

    pub fn save_observation(&self, obs: &Observation) -> Result<()> {
        let conn = lock_conn(&self.conn)?;
        conn.execute(
            r#"INSERT INTO observations 
               (id, session_id, observation_type, title, subtitle, narrative, facts, concepts, 
                files_read, files_modified, keywords, prompt_number, discovery_tokens, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"#,
            params![
                obs.id,
                obs.session_id,
                serde_json::to_string(&obs.observation_type)?,
                obs.title,
                obs.subtitle,
                obs.narrative,
                serde_json::to_string(&obs.facts)?,
                serde_json::to_string(&obs.concepts)?,
                serde_json::to_string(&obs.files_read)?,
                serde_json::to_string(&obs.files_modified)?,
                serde_json::to_string(&obs.keywords)?,
                obs.prompt_number,
                obs.discovery_tokens,
                obs.created_at.with_timezone(&Utc).to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<Observation>> {
        let conn = lock_conn(&self.conn)?;
        let mut stmt = conn.prepare(
            r#"SELECT id, session_id, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, created_at
               FROM observations WHERE id = ?1"#,
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            let obs_type_str: String = row.get(2)?;
            let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let facts: Vec<String> = serde_json::from_str(&row.get::<_, String>(6)?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let concepts: Vec<opencode_mem_core::Concept> =
                serde_json::from_str(&row.get::<_, String>(7)?)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let files_read: Vec<String> = serde_json::from_str(&row.get::<_, String>(8)?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let files_modified: Vec<String> = serde_json::from_str(&row.get::<_, String>(9)?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let keywords: Vec<String> = serde_json::from_str(&row.get::<_, String>(10)?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let created_at_str: String = row.get(13)?;
            let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                .with_timezone(&Utc);

            Ok(Some(Observation {
                id: row.get(0)?,
                session_id: row.get(1)?,
                observation_type: obs_type,
                title: row.get(3)?,
                subtitle: row.get(4)?,
                narrative: row.get(5)?,
                facts,
                concepts,
                files_read,
                files_modified,
                keywords,
                prompt_number: row.get(11)?,
                discovery_tokens: row.get(12)?,
                created_at,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_recent(&self, limit: usize) -> Result<Vec<SearchResult>> {
        let conn = lock_conn(&self.conn)?;
        let mut stmt = conn.prepare(
            r#"SELECT id, title, subtitle, observation_type
               FROM observations ORDER BY created_at DESC LIMIT ?1"#,
        )?;
        let results = stmt
            .query_map(params![limit], |row| {
                Ok(SearchResult {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    subtitle: row.get(2)?,
                    observation_type: serde_json::from_str(&row.get::<_, String>(3)?)
                        .unwrap_or(ObservationType::Change),
                    score: 1.0,
                })
            })?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub fn get_session_observations(&self, session_id: &str) -> Result<Vec<Observation>> {
        let conn = lock_conn(&self.conn)?;
        let mut stmt = conn.prepare(
            r#"SELECT id, session_id, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, created_at
               FROM observations WHERE session_id = ?1 ORDER BY created_at"#,
        )?;
        let results = stmt
            .query_map(params![session_id], |row| {
                let obs_type_str: String = row.get(2)?;
                let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

                let created_at_str: String = row.get(13)?;
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                    .with_timezone(&Utc);

                Ok(Observation {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    observation_type: obs_type,
                    title: row.get(3)?,
                    subtitle: row.get(4)?,
                    narrative: row.get(5)?,
                    facts: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                    concepts: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
                    files_read: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default(),
                    files_modified: serde_json::from_str(&row.get::<_, String>(9)?)
                        .unwrap_or_default(),
                    keywords: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default(),
                    prompt_number: row.get(11)?,
                    discovery_tokens: row.get(12)?,
                    created_at,
                })
            })?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let conn = lock_conn(&self.conn)?;
        let mut stmt = conn.prepare(
            r#"SELECT o.id, o.title, o.subtitle, o.observation_type, bm25(observations_fts) as score
               FROM observations_fts f
               JOIN observations o ON o.rowid = f.rowid
               WHERE observations_fts MATCH ?1
               ORDER BY score
               LIMIT ?2"#,
        )?;
        let results = stmt
            .query_map(params![query, limit], |row| {
                Ok(SearchResult {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    subtitle: row.get(2)?,
                    observation_type: serde_json::from_str(&row.get::<_, String>(3)?)
                        .unwrap_or(ObservationType::Change),
                    score: row.get(4)?,
                })
            })?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub fn hybrid_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let keywords: HashSet<String> =
            query.split_whitespace().map(|s| s.to_lowercase()).collect();

        let fts_query = query
            .split_whitespace()
            .map(|word| format!("\"{}\"*", word.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" AND ");

        if fts_query.is_empty() {
            return self.get_recent(limit);
        }

        let conn = lock_conn(&self.conn)?;
        let mut stmt = conn.prepare(
            r#"SELECT o.id, o.title, o.subtitle, o.observation_type, o.keywords,
                      bm25(observations_fts) as fts_score
               FROM observations_fts f
               JOIN observations o ON o.rowid = f.rowid
               WHERE observations_fts MATCH ?1
               LIMIT ?2"#,
        )?;

        let results: Vec<(SearchResult, f64)> = stmt
            .query_map(params![fts_query, limit * 2], |row| {
                let obs_keywords: String = row.get(4)?;
                let fts_score: f64 = row.get(5)?;
                Ok((
                    SearchResult {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        subtitle: row.get(2)?,
                        observation_type: serde_json::from_str(&row.get::<_, String>(3)?)
                            .unwrap_or(ObservationType::Change),
                        score: 0.0,
                    },
                    fts_score,
                    obs_keywords,
                ))
            })?
            .filter_map(log_row_error)
            .map(|(mut result, fts_score, obs_keywords)| {
                let obs_kw: HashSet<String> = serde_json::from_str::<Vec<String>>(&obs_keywords)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|s| s.to_lowercase())
                    .collect();
                let keyword_overlap = keywords.intersection(&obs_kw).count() as f64;
                let keyword_score = if keywords.is_empty() {
                    0.0
                } else {
                    keyword_overlap / keywords.len() as f64
                };
                result.score = (fts_score.abs() * 0.7) + (keyword_score * 0.3);
                let score = result.score;
                (result, score)
            })
            .collect();

        let mut results = results;
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results.into_iter().take(limit).map(|(r, _)| r).collect())
    }

    pub fn get_timeline(
        &self,
        from: Option<&str>,
        to: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let conn = lock_conn(&self.conn)?;

        let map_row = |row: &rusqlite::Row| -> rusqlite::Result<SearchResult> {
            Ok(SearchResult {
                id: row.get(0)?,
                title: row.get(1)?,
                subtitle: row.get(2)?,
                observation_type: serde_json::from_str(&row.get::<_, String>(3)?)
                    .unwrap_or(ObservationType::Change),
                score: 1.0,
            })
        };

        let results: Vec<SearchResult> = match (from, to) {
            (Some(f), Some(t)) => {
                let mut stmt = conn.prepare(
                    "SELECT id, title, subtitle, observation_type FROM observations WHERE created_at >= ?1 AND created_at <= ?2 ORDER BY created_at DESC LIMIT ?3"
                )?;
                let res = stmt
                    .query_map(params![f, t, limit], map_row)?
                    .filter_map(log_row_error)
                    .collect();
                res
            }
            (Some(f), None) => {
                let mut stmt = conn.prepare(
                    "SELECT id, title, subtitle, observation_type FROM observations WHERE created_at >= ?1 ORDER BY created_at DESC LIMIT ?2"
                )?;
                let res = stmt
                    .query_map(params![f, limit], map_row)?
                    .filter_map(log_row_error)
                    .collect();
                res
            }
            (None, Some(t)) => {
                let mut stmt = conn.prepare(
                    "SELECT id, title, subtitle, observation_type FROM observations WHERE created_at <= ?1 ORDER BY created_at DESC LIMIT ?2"
                )?;
                let res = stmt
                    .query_map(params![t, limit], map_row)?
                    .filter_map(log_row_error)
                    .collect();
                res
            }
            (None, None) => {
                let mut stmt = conn.prepare(
                    "SELECT id, title, subtitle, observation_type FROM observations ORDER BY created_at DESC LIMIT ?1"
                )?;
                let res = stmt
                    .query_map(params![limit], map_row)?
                    .filter_map(log_row_error)
                    .collect();
                res
            }
        };

        Ok(results)
    }

    pub fn save_summary(&self, summary: &SessionSummary) -> Result<()> {
        let conn = lock_conn(&self.conn)?;
        conn.execute(
            r#"INSERT OR REPLACE INTO session_summaries 
               (session_id, project, request, investigated, learned, completed, next_steps, notes, 
                prompt_number, discovery_tokens, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"#,
            params![
                summary.session_id,
                summary.project,
                summary.request,
                summary.investigated,
                summary.learned,
                summary.completed,
                summary.next_steps,
                summary.notes,
                summary.prompt_number,
                summary.discovery_tokens,
                summary.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_session_summary(&self, session_id: &str) -> Result<Option<SessionSummary>> {
        let conn = lock_conn(&self.conn)?;
        let mut stmt = conn.prepare(
            r#"SELECT session_id, project, request, investigated, learned, completed, next_steps, notes, 
                      prompt_number, discovery_tokens, created_at
               FROM session_summaries WHERE session_id = ?1"#,
        )?;
        let mut rows = stmt.query(params![session_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(SessionSummary {
                session_id: row.get(0)?,
                project: row.get(1)?,
                request: row.get(2)?,
                investigated: row.get(3)?,
                learned: row.get(4)?,
                completed: row.get(5)?,
                next_steps: row.get(6)?,
                notes: row.get(7)?,
                prompt_number: row.get(8)?,
                discovery_tokens: row.get(9)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)?
                    .with_timezone(&Utc),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn save_user_prompt(&self, prompt: &UserPrompt) -> Result<()> {
        let conn = lock_conn(&self.conn)?;
        conn.execute(
            r#"INSERT OR REPLACE INTO user_prompts 
               (id, content_session_id, prompt_number, prompt_text, project, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
            params![
                prompt.id,
                prompt.content_session_id,
                prompt.prompt_number,
                prompt.prompt_text,
                prompt.project,
                prompt.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn update_session_summary(&self, session_id: &str, summary: &str) -> Result<()> {
        let conn = lock_conn(&self.conn)?;
        conn.execute(
            "UPDATE sessions SET summary = ?1 WHERE id = ?2",
            params![summary, session_id],
        )?;
        Ok(())
    }
}
