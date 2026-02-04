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
use crate::types::{PaginatedResult, PendingMessage, PendingMessageStatus, StorageStats};

pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

fn lock_conn<T>(mutex: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|e: PoisonError<_>| anyhow::anyhow!("Database lock poisoned: {}", e))
}

/// Parse JSON with proper error propagation instead of silent fallback
fn parse_json<T: serde::de::DeserializeOwned>(s: &str) -> rusqlite::Result<T> {
    serde_json::from_str(s).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
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

fn escape_like_pattern(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
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
               (id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                files_read, files_modified, keywords, prompt_number, discovery_tokens, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"#,
            params![
                obs.id,
                obs.session_id,
                obs.project,
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
            r#"SELECT id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, created_at
               FROM observations WHERE id = ?1"#,
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            let obs_type_str: String = row.get(3)?;
            let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let facts: Vec<String> = serde_json::from_str(&row.get::<_, String>(7)?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let concepts: Vec<opencode_mem_core::Concept> =
                serde_json::from_str(&row.get::<_, String>(8)?)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let files_read: Vec<String> = serde_json::from_str(&row.get::<_, String>(9)?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let files_modified: Vec<String> = serde_json::from_str(&row.get::<_, String>(10)?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let keywords: Vec<String> = serde_json::from_str(&row.get::<_, String>(11)?)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

            let created_at_str: String = row.get(14)?;
            let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                .with_timezone(&Utc);

            Ok(Some(Observation {
                id: row.get(0)?,
                session_id: row.get(1)?,
                project: row.get(2)?,
                observation_type: obs_type,
                title: row.get(4)?,
                subtitle: row.get(5)?,
                narrative: row.get(6)?,
                facts,
                concepts,
                files_read,
                files_modified,
                keywords,
                prompt_number: row.get(12)?,
                discovery_tokens: row.get(13)?,
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
                    observation_type: parse_json(&row.get::<_, String>(3)?)?,
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
            r#"SELECT id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, created_at
               FROM observations WHERE session_id = ?1 ORDER BY created_at"#,
        )?;
        let results = stmt
            .query_map(params![session_id], |row| {
                let obs_type_str: String = row.get(3)?;
                let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

                let created_at_str: String = row.get(14)?;
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                    .with_timezone(&Utc);

                Ok(Observation {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    project: row.get(2)?,
                    observation_type: obs_type,
                    title: row.get(4)?,
                    subtitle: row.get(5)?,
                    narrative: row.get(6)?,
                    facts: parse_json(&row.get::<_, String>(7)?)?,
                    concepts: parse_json(&row.get::<_, String>(8)?)?,
                    files_read: parse_json(&row.get::<_, String>(9)?)?,
                    files_modified: parse_json(&row.get::<_, String>(10)?)?,
                    keywords: parse_json(&row.get::<_, String>(11)?)?,
                    prompt_number: row.get(12)?,
                    discovery_tokens: row.get(13)?,
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
                    observation_type: parse_json(&row.get::<_, String>(3)?)?,
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
                        observation_type: parse_json(&row.get::<_, String>(3)?)?,
                        score: 0.0,
                    },
                    fts_score,
                    obs_keywords,
                ))
            })?
            .filter_map(log_row_error)
            .filter_map(|(mut result, fts_score, obs_keywords)| {
                let obs_kw: HashSet<String> = match parse_json::<Vec<String>>(&obs_keywords) {
                    Ok(v) => v.into_iter().map(|s| s.to_lowercase()).collect(),
                    Err(e) => {
                        tracing::warn!("Failed to parse keywords JSON: {}", e);
                        return None;
                    }
                };
                let keyword_overlap = keywords.intersection(&obs_kw).count() as f64;
                let keyword_score = if keywords.is_empty() {
                    0.0
                } else {
                    keyword_overlap / keywords.len() as f64
                };
                result.score = (fts_score.abs() * 0.7) + (keyword_score * 0.3);
                let score = result.score;
                Some((result, score))
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
                observation_type: parse_json(&row.get::<_, String>(3)?)?,
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
                files_read, files_edited, prompt_number, discovery_tokens, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"#,
            params![
                summary.session_id,
                summary.project,
                summary.request,
                summary.investigated,
                summary.learned,
                summary.completed,
                summary.next_steps,
                summary.notes,
                serde_json::to_string(&summary.files_read)?,
                serde_json::to_string(&summary.files_edited)?,
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
                      files_read, files_edited, prompt_number, discovery_tokens, created_at
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
                files_read: parse_json(&row.get::<_, String>(8)?)?,
                files_edited: parse_json(&row.get::<_, String>(9)?)?,
                prompt_number: row.get(10)?,
                discovery_tokens: row.get(11)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)?
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

    pub fn update_session_status_with_summary(
        &self,
        session_id: &str,
        status: SessionStatus,
        summary: Option<&str>,
    ) -> Result<()> {
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
                session_id
            ],
        )?;

        // If summary provided, fetch session project and save summary inline
        // (avoid calling self.get_session/self.save_summary to prevent deadlock)
        if let Some(s) = summary {
            let project: Option<String> = conn
                .query_row(
                    "SELECT project FROM sessions WHERE id = ?1",
                    params![session_id],
                    |row| row.get(0),
                )
                .ok();

            if let Some(proj) = project {
                let now = Utc::now();
                conn.execute(
                    r#"INSERT OR REPLACE INTO session_summaries 
                       (session_id, project, request, investigated, learned, completed, next_steps, notes, 
                        files_read, files_edited, prompt_number, discovery_tokens, created_at)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"#,
                    params![
                        session_id,
                        proj,
                        Option::<String>::None,
                        Option::<String>::None,
                        Some(s),
                        Option::<String>::None,
                        Option::<String>::None,
                        Option::<String>::None,
                        "[]",
                        "[]",
                        Option::<i32>::None,
                        Option::<i32>::None,
                        now.to_rfc3339(),
                    ],
                )?;
            }
        }
        Ok(())
    }

    pub fn get_observations_by_ids(&self, ids: &[String]) -> Result<Vec<Observation>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = lock_conn(&self.conn)?;
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            r#"SELECT id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, created_at
               FROM observations WHERE id IN ({}) ORDER BY created_at DESC"#,
            placeholders
        );
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let results = stmt
            .query_map(params.as_slice(), |row| {
                let obs_type_str: String = row.get(3)?;
                let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                let created_at_str: String = row.get(14)?;
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                    .with_timezone(&Utc);
                Ok(Observation {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    project: row.get(2)?,
                    observation_type: obs_type,
                    title: row.get(4)?,
                    subtitle: row.get(5)?,
                    narrative: row.get(6)?,
                    facts: parse_json(&row.get::<_, String>(7)?)?,
                    concepts: parse_json(&row.get::<_, String>(8)?)?,
                    files_read: parse_json(&row.get::<_, String>(9)?)?,
                    files_modified: parse_json(&row.get::<_, String>(10)?)?,
                    keywords: parse_json(&row.get::<_, String>(11)?)?,
                    prompt_number: row.get(12)?,
                    discovery_tokens: row.get(13)?,
                    created_at,
                })
            })?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub fn get_all_projects(&self) -> Result<Vec<String>> {
        let conn = lock_conn(&self.conn)?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT project FROM observations WHERE project IS NOT NULL ORDER BY project",
        )?;
        let results = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub fn get_stats(&self) -> Result<StorageStats> {
        let conn = lock_conn(&self.conn)?;
        let observation_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM observations", [], |row| row.get(0))?;
        let session_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
        let summary_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM session_summaries", [], |row| {
                row.get(0)
            })?;
        let prompt_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM user_prompts", [], |row| row.get(0))?;
        let project_count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT project) FROM observations WHERE project IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(StorageStats {
            observation_count: observation_count as u64,
            session_count: session_count as u64,
            summary_count: summary_count as u64,
            prompt_count: prompt_count as u64,
            project_count: project_count as u64,
        })
    }

    pub fn search_with_filters(
        &self,
        query: Option<&str>,
        project: Option<&str>,
        obs_type: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        let conn = lock_conn(&self.conn)?;

        let mut conditions = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(p) = project {
            conditions.push("o.project = ?".to_string());
            params_vec.push(Box::new(p.to_string()));
        }
        if let Some(t) = obs_type {
            conditions.push("o.observation_type = ?".to_string());
            params_vec.push(Box::new(format!("\"{}\"", t)));
        }

        if let Some(q) = query {
            if !q.is_empty() {
                let fts_query = q
                    .split_whitespace()
                    .map(|word| format!("\"{}\"*", word.replace('"', "")))
                    .collect::<Vec<_>>()
                    .join(" AND ");

                let where_clause = if conditions.is_empty() {
                    String::new()
                } else {
                    format!("AND {}", conditions.join(" AND "))
                };

                let sql = format!(
                    r#"SELECT o.id, o.title, o.subtitle, o.observation_type, bm25(observations_fts) as score
                       FROM observations_fts f
                       JOIN observations o ON o.rowid = f.rowid
                       WHERE observations_fts MATCH ? {}
                       ORDER BY score
                       LIMIT ?"#,
                    where_clause
                );

                let mut stmt = conn.prepare(&sql)?;
                let mut all_params: Vec<&dyn rusqlite::ToSql> =
                    vec![&fts_query as &dyn rusqlite::ToSql];
                for p in &params_vec {
                    all_params.push(p.as_ref());
                }
                all_params.push(&limit);

                let results = stmt
                    .query_map(all_params.as_slice(), |row| {
                        Ok(SearchResult {
                            id: row.get(0)?,
                            title: row.get(1)?,
                            subtitle: row.get(2)?,
                            observation_type: parse_json(&row.get::<_, String>(3)?)?,
                            score: row.get(4)?,
                        })
                    })?
                    .filter_map(log_row_error)
                    .collect();
                return Ok(results);
            }
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            r#"SELECT id, title, subtitle, observation_type
               FROM observations o {} ORDER BY created_at DESC LIMIT ?"#,
            where_clause
        );

        let mut stmt = conn.prepare(&sql)?;
        let mut all_params: Vec<&dyn rusqlite::ToSql> = Vec::new();
        for p in &params_vec {
            all_params.push(p.as_ref());
        }
        all_params.push(&limit);

        let results = stmt
            .query_map(all_params.as_slice(), |row| {
                Ok(SearchResult {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    subtitle: row.get(2)?,
                    observation_type: parse_json(&row.get::<_, String>(3)?)?,
                    score: 1.0,
                })
            })?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    /// Get paginated observations with optional project filter
    pub fn get_observations_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<Observation>> {
        let conn = lock_conn(&self.conn)?;

        let total: i64 = if let Some(p) = project {
            conn.query_row(
                "SELECT COUNT(*) FROM observations WHERE project = ?1",
                params![p],
                |row| row.get(0),
            )?
        } else {
            conn.query_row("SELECT COUNT(*) FROM observations", [], |row| row.get(0))?
        };

        let sql = if project.is_some() {
            r#"SELECT id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, created_at
               FROM observations WHERE project = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"#
        } else {
            r#"SELECT id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, created_at
               FROM observations ORDER BY created_at DESC LIMIT ?1 OFFSET ?2"#
        };

        let mut stmt = conn.prepare(sql)?;
        let items: Vec<Observation> = if let Some(p) = project {
            stmt.query_map(params![p, limit, offset], |row| {
                let obs_type_str: String = row.get(3)?;
                let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                let created_at_str: String = row.get(14)?;
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                    .with_timezone(&Utc);
                Ok(Observation {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    project: row.get(2)?,
                    observation_type: obs_type,
                    title: row.get(4)?,
                    subtitle: row.get(5)?,
                    narrative: row.get(6)?,
                    facts: parse_json(&row.get::<_, String>(7)?)?,
                    concepts: parse_json(&row.get::<_, String>(8)?)?,
                    files_read: parse_json(&row.get::<_, String>(9)?)?,
                    files_modified: parse_json(&row.get::<_, String>(10)?)?,
                    keywords: parse_json(&row.get::<_, String>(11)?)?,
                    prompt_number: row.get(12)?,
                    discovery_tokens: row.get(13)?,
                    created_at,
                })
            })?
            .filter_map(log_row_error)
            .collect()
        } else {
            stmt.query_map(params![limit, offset], |row| {
                let obs_type_str: String = row.get(3)?;
                let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                let created_at_str: String = row.get(14)?;
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                    .with_timezone(&Utc);
                Ok(Observation {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    project: row.get(2)?,
                    observation_type: obs_type,
                    title: row.get(4)?,
                    subtitle: row.get(5)?,
                    narrative: row.get(6)?,
                    facts: parse_json(&row.get::<_, String>(7)?)?,
                    concepts: parse_json(&row.get::<_, String>(8)?)?,
                    files_read: parse_json(&row.get::<_, String>(9)?)?,
                    files_modified: parse_json(&row.get::<_, String>(10)?)?,
                    keywords: parse_json(&row.get::<_, String>(11)?)?,
                    prompt_number: row.get(12)?,
                    discovery_tokens: row.get(13)?,
                    created_at,
                })
            })?
            .filter_map(log_row_error)
            .collect()
        };

        Ok(PaginatedResult {
            items,
            total: total as u64,
            offset: offset as u64,
            limit: limit as u64,
        })
    }

    /// Get paginated session summaries with optional project filter
    pub fn get_summaries_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<SessionSummary>> {
        let conn = lock_conn(&self.conn)?;

        let total: i64 = if let Some(p) = project {
            conn.query_row(
                "SELECT COUNT(*) FROM session_summaries WHERE project = ?1",
                params![p],
                |row| row.get(0),
            )?
        } else {
            conn.query_row("SELECT COUNT(*) FROM session_summaries", [], |row| {
                row.get(0)
            })?
        };

        let sql = if project.is_some() {
            r#"SELECT session_id, project, request, investigated, learned, completed, next_steps, notes, 
                      files_read, files_edited, prompt_number, discovery_tokens, created_at
               FROM session_summaries WHERE project = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"#
        } else {
            r#"SELECT session_id, project, request, investigated, learned, completed, next_steps, notes, 
                      files_read, files_edited, prompt_number, discovery_tokens, created_at
               FROM session_summaries ORDER BY created_at DESC LIMIT ?1 OFFSET ?2"#
        };

        let mut stmt = conn.prepare(sql)?;
        let items: Vec<SessionSummary> = if let Some(p) = project {
            stmt.query_map(params![p, limit, offset], |row| {
                Ok(SessionSummary {
                    session_id: row.get(0)?,
                    project: row.get(1)?,
                    request: row.get(2)?,
                    investigated: row.get(3)?,
                    learned: row.get(4)?,
                    completed: row.get(5)?,
                    next_steps: row.get(6)?,
                    notes: row.get(7)?,
                    files_read: parse_json(&row.get::<_, String>(8)?)?,
                    files_edited: parse_json(&row.get::<_, String>(9)?)?,
                    prompt_number: row.get(10)?,
                    discovery_tokens: row.get(11)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                        .with_timezone(&Utc),
                })
            })?
            .filter_map(log_row_error)
            .collect()
        } else {
            stmt.query_map(params![limit, offset], |row| {
                Ok(SessionSummary {
                    session_id: row.get(0)?,
                    project: row.get(1)?,
                    request: row.get(2)?,
                    investigated: row.get(3)?,
                    learned: row.get(4)?,
                    completed: row.get(5)?,
                    next_steps: row.get(6)?,
                    notes: row.get(7)?,
                    files_read: parse_json(&row.get::<_, String>(8)?)?,
                    files_edited: parse_json(&row.get::<_, String>(9)?)?,
                    prompt_number: row.get(10)?,
                    discovery_tokens: row.get(11)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                        .with_timezone(&Utc),
                })
            })?
            .filter_map(log_row_error)
            .collect()
        };

        Ok(PaginatedResult {
            items,
            total: total as u64,
            offset: offset as u64,
            limit: limit as u64,
        })
    }

    /// Get paginated user prompts with optional project filter
    pub fn get_prompts_paginated(
        &self,
        offset: usize,
        limit: usize,
        project: Option<&str>,
    ) -> Result<PaginatedResult<UserPrompt>> {
        let conn = lock_conn(&self.conn)?;

        let total: i64 = if let Some(p) = project {
            conn.query_row(
                "SELECT COUNT(*) FROM user_prompts WHERE project = ?1",
                params![p],
                |row| row.get(0),
            )?
        } else {
            conn.query_row("SELECT COUNT(*) FROM user_prompts", [], |row| row.get(0))?
        };

        let sql = if project.is_some() {
            r#"SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
               FROM user_prompts WHERE project = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"#
        } else {
            r#"SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
               FROM user_prompts ORDER BY created_at DESC LIMIT ?1 OFFSET ?2"#
        };

        let mut stmt = conn.prepare(sql)?;
        let items: Vec<UserPrompt> = if let Some(p) = project {
            stmt.query_map(params![p, limit, offset], |row| {
                Ok(UserPrompt {
                    id: row.get(0)?,
                    content_session_id: row.get(1)?,
                    prompt_number: row.get(2)?,
                    prompt_text: row.get(3)?,
                    project: row.get(4)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                        .with_timezone(&Utc),
                })
            })?
            .filter_map(log_row_error)
            .collect()
        } else {
            stmt.query_map(params![limit, offset], |row| {
                Ok(UserPrompt {
                    id: row.get(0)?,
                    content_session_id: row.get(1)?,
                    prompt_number: row.get(2)?,
                    prompt_text: row.get(3)?,
                    project: row.get(4)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                        .with_timezone(&Utc),
                })
            })?
            .filter_map(log_row_error)
            .collect()
        };

        Ok(PaginatedResult {
            items,
            total: total as u64,
            offset: offset as u64,
            limit: limit as u64,
        })
    }

    /// Get user prompt by ID
    pub fn get_prompt_by_id(&self, id: &str) -> Result<Option<UserPrompt>> {
        let conn = lock_conn(&self.conn)?;
        let mut stmt = conn.prepare(
            r#"SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
               FROM user_prompts WHERE id = ?1"#,
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(UserPrompt {
                id: row.get(0)?,
                content_session_id: row.get(1)?,
                prompt_number: row.get(2)?,
                prompt_text: row.get(3)?,
                project: row.get(4)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                    .map_err(|e| anyhow::anyhow!("Failed to parse date: {}", e))?
                    .with_timezone(&Utc),
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_context_for_project(&self, project: &str, limit: usize) -> Result<Vec<Observation>> {
        let conn = lock_conn(&self.conn)?;
        let mut stmt = conn.prepare(
            r#"SELECT id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, created_at
               FROM observations WHERE project = ?1 ORDER BY created_at DESC LIMIT ?2"#,
        )?;
        let results = stmt
            .query_map(params![project, limit], |row| {
                let obs_type_str: String = row.get(3)?;
                let obs_type: ObservationType = serde_json::from_str(&obs_type_str)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                let created_at_str: String = row.get(14)?;
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                    .with_timezone(&Utc);
                Ok(Observation {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    project: row.get(2)?,
                    observation_type: obs_type,
                    title: row.get(4)?,
                    subtitle: row.get(5)?,
                    narrative: row.get(6)?,
                    facts: parse_json(&row.get::<_, String>(7)?)?,
                    concepts: parse_json(&row.get::<_, String>(8)?)?,
                    files_read: parse_json(&row.get::<_, String>(9)?)?,
                    files_modified: parse_json(&row.get::<_, String>(10)?)?,
                    keywords: parse_json(&row.get::<_, String>(11)?)?,
                    prompt_number: row.get(12)?,
                    discovery_tokens: row.get(13)?,
                    created_at,
                })
            })?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub fn search_sessions(&self, query: &str, limit: usize) -> Result<Vec<SessionSummary>> {
        let conn = lock_conn(&self.conn)?;
        let fts_query = query
            .split_whitespace()
            .map(|word| format!("\"{}\"*", word.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" AND ");

        if fts_query.is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = conn.prepare(
            r#"SELECT s.session_id, s.project, s.request, s.investigated, s.learned, s.completed, 
                      s.next_steps, s.notes, s.files_read, s.files_edited, s.prompt_number, 
                      s.discovery_tokens, s.created_at
               FROM summaries_fts f
               JOIN session_summaries s ON s.rowid = f.rowid
               WHERE summaries_fts MATCH ?1
               ORDER BY bm25(summaries_fts)
               LIMIT ?2"#,
        )?;
        let results = stmt
            .query_map(params![fts_query, limit], |row| {
                Ok(SessionSummary {
                    session_id: row.get(0)?,
                    project: row.get(1)?,
                    request: row.get(2)?,
                    investigated: row.get(3)?,
                    learned: row.get(4)?,
                    completed: row.get(5)?,
                    next_steps: row.get(6)?,
                    notes: row.get(7)?,
                    files_read: parse_json(&row.get::<_, String>(8)?)?,
                    files_edited: parse_json(&row.get::<_, String>(9)?)?,
                    prompt_number: row.get(10)?,
                    discovery_tokens: row.get(11)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                        .with_timezone(&Utc),
                })
            })?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub fn search_prompts(&self, query: &str, limit: usize) -> Result<Vec<UserPrompt>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let conn = lock_conn(&self.conn)?;
        let escaped_query = escape_like_pattern(query);
        let pattern = format!("%{}%", escaped_query);
        let mut stmt = conn.prepare(
            r#"SELECT id, content_session_id, prompt_number, prompt_text, project, created_at
               FROM user_prompts
               WHERE prompt_text LIKE ?1 ESCAPE '\'
               ORDER BY created_at DESC
               LIMIT ?2"#,
        )?;
        let results = stmt
            .query_map(params![pattern, limit], |row| {
                Ok(UserPrompt {
                    id: row.get(0)?,
                    content_session_id: row.get(1)?,
                    prompt_number: row.get(2)?,
                    prompt_text: row.get(3)?,
                    project: row.get(4)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                        .with_timezone(&Utc),
                })
            })?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub fn search_by_file(&self, file_path: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let conn = lock_conn(&self.conn)?;
        let escaped = escape_like_pattern(file_path);
        let pattern = format!("%{}%", escaped);
        let mut stmt = conn.prepare(
            r#"SELECT id, title, subtitle, observation_type
               FROM observations
               WHERE files_read LIKE ?1 ESCAPE '\' OR files_modified LIKE ?1 ESCAPE '\'
               ORDER BY created_at DESC
               LIMIT ?2"#,
        )?;
        let results = stmt
            .query_map(params![pattern, limit], |row| {
                Ok(SearchResult {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    subtitle: row.get(2)?,
                    observation_type: parse_json(&row.get::<_, String>(3)?)?,
                    score: 1.0,
                })
            })?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Pending Queue Methods
    // ─────────────────────────────────────────────────────────────────────────

    /// Queue a new message for processing
    pub fn queue_message(
        &self,
        session_id: &str,
        tool_name: Option<&str>,
        tool_input: Option<&str>,
        tool_response: Option<&str>,
    ) -> Result<i64> {
        let conn = lock_conn(&self.conn)?;
        let now = Utc::now().timestamp();
        conn.execute(
            r#"INSERT INTO pending_messages 
               (session_id, status, tool_name, tool_input, tool_response, retry_count, created_at_epoch)
               VALUES (?1, 'pending', ?2, ?3, ?4, 0, ?5)"#,
            params![session_id, tool_name, tool_input, tool_response, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Claim pending messages for processing (visibility timeout pattern)
    /// Uses atomic UPDATE with RETURNING to prevent race conditions
    pub fn claim_pending_messages(
        &self,
        limit: usize,
        visibility_timeout_secs: i64,
    ) -> Result<Vec<PendingMessage>> {
        let conn = lock_conn(&self.conn)?;
        let now = Utc::now().timestamp();
        let stale_threshold = now - visibility_timeout_secs;

        // Atomic claim: UPDATE and RETURN in single statement
        let mut stmt = conn.prepare(
            r#"UPDATE pending_messages
               SET status = 'processing', claimed_at_epoch = ?1
               WHERE id IN (
                   SELECT id FROM pending_messages
                   WHERE status = 'pending'
                      OR (status = 'processing' AND claimed_at_epoch < ?2)
                   ORDER BY created_at_epoch ASC
                   LIMIT ?3
               )
               RETURNING id, session_id, status, tool_name, tool_input, tool_response,
                         retry_count, created_at_epoch, claimed_at_epoch, completed_at_epoch"#,
        )?;

        let messages: Vec<PendingMessage> = stmt
            .query_map(params![now, stale_threshold, limit], |row| {
                let status_str: String = row.get(2)?;
                let status = status_str
                    .parse::<PendingMessageStatus>()
                    .unwrap_or(PendingMessageStatus::Processing);
                Ok(PendingMessage {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    status,
                    tool_name: row.get(3)?,
                    tool_input: row.get(4)?,
                    tool_response: row.get(5)?,
                    retry_count: row.get(6)?,
                    created_at_epoch: row.get(7)?,
                    claimed_at_epoch: row.get(8)?,
                    completed_at_epoch: row.get(9)?,
                })
            })?
            .filter_map(log_row_error)
            .collect();

        Ok(messages)
    }

    /// Mark a message as completed
    pub fn complete_message(&self, id: i64) -> Result<()> {
        let conn = lock_conn(&self.conn)?;
        let now = Utc::now().timestamp();
        conn.execute(
            r#"UPDATE pending_messages 
               SET status = 'processed', completed_at_epoch = ?1
               WHERE id = ?2"#,
            params![now, id],
        )?;
        Ok(())
    }

    /// Mark a message as failed, optionally incrementing retry count.
    /// If retry count reaches MAX_RETRY_COUNT, status becomes 'failed'.
    /// Otherwise, status goes back to 'pending' for retry.
    pub fn fail_message(&self, id: i64, increment_retry: bool) -> Result<()> {
        let conn = lock_conn(&self.conn)?;
        if increment_retry {
            // Increment retry count and set status based on whether max retries reached
            conn.execute(
                r#"UPDATE pending_messages 
                   SET retry_count = retry_count + 1,
                       status = CASE 
                           WHEN retry_count + 1 >= ?1 THEN 'failed'
                           ELSE 'pending'
                       END,
                       claimed_at_epoch = NULL
                   WHERE id = ?2"#,
                params![crate::types::MAX_RETRY_COUNT, id],
            )?;
        } else {
            conn.execute(
                r#"UPDATE pending_messages SET status = 'failed' WHERE id = ?1"#,
                params![id],
            )?;
        }
        Ok(())
    }

    /// Get count of pending messages
    pub fn get_pending_count(&self) -> Result<usize> {
        let conn = lock_conn(&self.conn)?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pending_messages WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Release stale messages (processing for too long) back to pending
    pub fn release_stale_messages(&self, visibility_timeout_secs: i64) -> Result<usize> {
        let conn = lock_conn(&self.conn)?;
        let now = Utc::now().timestamp();
        let stale_threshold = now - visibility_timeout_secs;
        let affected = conn.execute(
            r#"UPDATE pending_messages 
               SET status = 'pending', claimed_at_epoch = NULL
               WHERE status = 'processing' AND claimed_at_epoch <= ?1"#,
            params![stale_threshold],
        )?;
        Ok(affected)
    }

    /// Get failed messages for inspection/retry
    pub fn get_failed_messages(&self, limit: usize) -> Result<Vec<PendingMessage>> {
        let conn = lock_conn(&self.conn)?;
        let mut stmt = conn.prepare(
            r#"SELECT id, session_id, status, tool_name, tool_input, tool_response,
                      retry_count, created_at_epoch, claimed_at_epoch, completed_at_epoch
               FROM pending_messages
               WHERE status = 'failed'
               ORDER BY created_at_epoch DESC
               LIMIT ?1"#,
        )?;
        let results = stmt
            .query_map(params![limit], |row| {
                let status_str: String = row.get(2)?;
                let status = status_str
                    .parse::<PendingMessageStatus>()
                    .unwrap_or(PendingMessageStatus::Failed);
                Ok(PendingMessage {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    status,
                    tool_name: row.get(3)?,
                    tool_input: row.get(4)?,
                    tool_response: row.get(5)?,
                    retry_count: row.get(6)?,
                    created_at_epoch: row.get(7)?,
                    claimed_at_epoch: row.get(8)?,
                    completed_at_epoch: row.get(9)?,
                })
            })?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub fn get_all_pending_messages(&self, limit: usize) -> Result<Vec<PendingMessage>> {
        let conn = lock_conn(&self.conn)?;
        let mut stmt = conn.prepare(
            r#"SELECT id, session_id, status, tool_name, tool_input, tool_response,
                      retry_count, created_at_epoch, claimed_at_epoch, completed_at_epoch
               FROM pending_messages
               ORDER BY created_at_epoch DESC
               LIMIT ?1"#,
        )?;
        let results = stmt
            .query_map(params![limit], |row| {
                let status_str: String = row.get(2)?;
                let status = status_str
                    .parse::<PendingMessageStatus>()
                    .unwrap_or(PendingMessageStatus::Pending);
                Ok(PendingMessage {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    status,
                    tool_name: row.get(3)?,
                    tool_input: row.get(4)?,
                    tool_response: row.get(5)?,
                    retry_count: row.get(6)?,
                    created_at_epoch: row.get(7)?,
                    claimed_at_epoch: row.get(8)?,
                    completed_at_epoch: row.get(9)?,
                })
            })?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub fn get_queue_stats(&self) -> Result<crate::types::QueueStats> {
        let conn = lock_conn(&self.conn)?;
        let (pending, processing, failed, processed): (
            Option<i64>,
            Option<i64>,
            Option<i64>,
            Option<i64>,
        ) = conn.query_row(
            r#"SELECT 
                SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'processing' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END),
                SUM(CASE WHEN status = 'processed' THEN 1 ELSE 0 END)
            FROM pending_messages"#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        Ok(crate::types::QueueStats {
            pending: pending.unwrap_or(0) as u64,
            processing: processing.unwrap_or(0) as u64,
            failed: failed.unwrap_or(0) as u64,
            processed: processed.unwrap_or(0) as u64,
        })
    }

    pub fn clear_failed_messages(&self) -> Result<usize> {
        let conn = lock_conn(&self.conn)?;
        let affected = conn.execute("DELETE FROM pending_messages WHERE status = 'failed'", [])?;
        Ok(affected)
    }

    pub fn clear_all_pending_messages(&self) -> Result<usize> {
        let conn = lock_conn(&self.conn)?;
        let affected = conn.execute("DELETE FROM pending_messages", [])?;
        Ok(affected)
    }

    pub fn get_session_observation_count(&self, session_id: &str) -> Result<usize> {
        let conn = lock_conn(&self.conn)?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM observations WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn delete_session(&self, session_id: &str) -> Result<bool> {
        let conn = lock_conn(&self.conn)?;
        let affected = conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])?;
        Ok(affected > 0)
    }

    pub fn get_session_by_content_id(&self, content_session_id: &str) -> Result<Option<Session>> {
        let conn = lock_conn(&self.conn)?;
        let mut stmt = conn.prepare(
            r#"SELECT id, content_session_id, memory_session_id, project, user_prompt, started_at, ended_at, status, prompt_counter
               FROM sessions WHERE content_session_id = ?1"#,
        )?;
        let mut rows = stmt.query(params![content_session_id])?;
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
}
