use anyhow::Result;
use chrono::Utc;
use opencode_mem_core::{
    compute_merge, DiscoveryTokens, NoiseLevel, Observation, ObservationType, PromptNumber,
    SearchResult,
};
use rusqlite::params;
use std::str::FromStr as _;

use super::{
    coerce_to_sql, escape_like_pattern, get_conn, log_row_error, parse_json,
    parse_observation_type, Storage,
};

impl Storage {
    /// Save observation to database. Returns `true` if inserted, `false` if
    /// duplicate title blocked the insert (UNIQUE constraint on `title_normalized`).
    ///
    /// # Errors
    /// Returns error if database operation fails.
    pub fn save_observation(&self, obs: &Observation) -> Result<bool> {
        let conn = get_conn(&self.pool)?;
        let rows = conn.execute(
            "INSERT OR IGNORE INTO observations 
               (id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                files_read, files_modified, keywords, prompt_number, discovery_tokens, noise_level, noise_reason, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                obs.id,
                obs.session_id,
                obs.project,
                obs.observation_type.as_str(),
                obs.title,
                obs.subtitle,
                obs.narrative,
                serde_json::to_string(&obs.facts)?,
                serde_json::to_string(&obs.concepts)?,
                serde_json::to_string(&obs.files_read)?,
                serde_json::to_string(&obs.files_modified)?,
                serde_json::to_string(&obs.keywords)?,
                obs.prompt_number.map(|v| v.0),
                obs.discovery_tokens.map(|v| v.0),
                obs.noise_level.as_str(),
                obs.noise_reason,
                obs.created_at.with_timezone(&Utc).to_rfc3339(),
            ],
        )?;
        Ok(rows > 0)
    }

    /// Get observation by ID.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_by_id(&self, id: &str) -> Result<Option<Observation>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, noise_level, noise_reason, created_at
               FROM observations WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_observation(row)?))
        } else {
            Ok(None)
        }
    }

    /// Get recent observations.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_recent(&self, limit: usize) -> Result<Vec<Observation>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, noise_level, noise_reason, created_at
               FROM observations ORDER BY created_at DESC LIMIT ?1",
        )?;
        let results = stmt
            .query_map(params![limit], Self::row_to_observation)?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    /// Get all observations for a session.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_session_observations(&self, session_id: &str) -> Result<Vec<Observation>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, noise_level, noise_reason, created_at
               FROM observations WHERE session_id = ?1 ORDER BY created_at",
        )?;
        let results = stmt
            .query_map(params![session_id], Self::row_to_observation)?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    /// Get observations by IDs.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_observations_by_ids(&self, ids: &[String]) -> Result<Vec<Observation>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = get_conn(&self.pool)?;
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, noise_level, noise_reason, created_at
               FROM observations WHERE id IN ({placeholders}) ORDER BY created_at DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(coerce_to_sql).collect();
        let results = stmt
            .query_map(params.as_slice(), Self::row_to_observation)?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    /// Get observations for a project.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_context_for_project(&self, project: &str, limit: usize) -> Result<Vec<Observation>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, 
                      files_read, files_modified, keywords, prompt_number, discovery_tokens, noise_level, noise_reason, created_at
               FROM observations WHERE project = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let results = stmt
            .query_map(params![project, limit], Self::row_to_observation)?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    /// Count observations in a session.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_session_observation_count(&self, session_id: &str) -> Result<usize> {
        let conn = get_conn(&self.pool)?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM observations WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Search observations by file path.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn search_by_file(&self, file_path: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let conn = get_conn(&self.pool)?;
        let escaped = escape_like_pattern(file_path);
        let pattern = format!("%{escaped}%");
        let mut stmt = conn.prepare(
            r"SELECT id, title, subtitle, observation_type, noise_level
               FROM observations
               WHERE files_read LIKE ?1 ESCAPE '\' OR files_modified LIKE ?1 ESCAPE '\'
               ORDER BY created_at DESC
               LIMIT ?2",
        )?;
        let results = stmt
            .query_map(params![pattern, limit], |row| {
                let noise_str: Option<String> = row.get(4)?;
                let noise_level = match noise_str {
                    Some(s) => NoiseLevel::from_str(&s).unwrap_or_else(|_| {
                        tracing::warn!(invalid_level = %s, "corrupt noise_level in DB, defaulting");
                        NoiseLevel::default()
                    }),
                    None => NoiseLevel::default(),
                };
                Ok(SearchResult::new(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    parse_observation_type(&row.get::<_, String>(3)?)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?,
                    noise_level,
                    0.0,
                ))
            })?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    pub(crate) fn row_to_observation(row: &rusqlite::Row<'_>) -> rusqlite::Result<Observation> {
        let obs_type_str: String = row.get(3)?;
        let obs_type: ObservationType = parse_observation_type(&obs_type_str)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
        let noise_str: Option<String> = row.get(14)?;
        let noise_level = match noise_str {
            Some(s) => NoiseLevel::from_str(&s).unwrap_or_else(|_| {
                tracing::warn!(invalid_level = %s, "corrupt noise_level in DB, defaulting");
                NoiseLevel::default()
            }),
            None => NoiseLevel::default(),
        };
        let noise_reason: Option<String> = row.get(15)?;
        let created_at_str: String = row.get(16)?;
        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
            .with_timezone(&Utc);
        Ok(Observation::builder(row.get(0)?, row.get(1)?, obs_type, row.get(4)?)
            .maybe_project(row.get(2)?)
            .maybe_subtitle(row.get(5)?)
            .maybe_narrative(row.get(6)?)
            .facts(parse_json(&row.get::<_, String>(7)?)?)
            .concepts(parse_json(&row.get::<_, String>(8)?)?)
            .files_read(parse_json(&row.get::<_, String>(9)?)?)
            .files_modified(parse_json(&row.get::<_, String>(10)?)?)
            .keywords(parse_json(&row.get::<_, String>(11)?)?)
            .maybe_prompt_number(row.get::<_, Option<u32>>(12)?.map(PromptNumber))
            .maybe_discovery_tokens(row.get::<_, Option<u32>>(13)?.map(DiscoveryTokens))
            .noise_level(noise_level)
            .maybe_noise_reason(noise_reason)
            .created_at(created_at)
            .build())
    }

    pub fn merge_into_existing(&self, existing_id: &str, newer: &Observation) -> Result<()> {
        let conn = get_conn(&self.pool)?;
        let tx = conn.unchecked_transaction()?;
        let existing = {
            let mut stmt = tx.prepare(
                "SELECT id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts,
                        files_read, files_modified, keywords, prompt_number, discovery_tokens, noise_level, noise_reason, created_at
                   FROM observations WHERE id = ?1",
            )?;
            let mut rows = stmt.query(params![existing_id])?;
            match rows.next()? {
                Some(row) => Self::row_to_observation(row)?,
                None => return Err(anyhow::anyhow!("observation not found: {existing_id}")),
            }
        };

        let merged = compute_merge(&existing, newer);

        tx.execute(
            "UPDATE observations SET facts = ?1, keywords = ?2, files_read = ?3,
                    files_modified = ?4, narrative = ?5, created_at = ?6, concepts = ?7,
                    noise_level = ?8, subtitle = ?9
               WHERE id = ?10",
            params![
                serde_json::to_string(&merged.facts)?,
                serde_json::to_string(&merged.keywords)?,
                serde_json::to_string(&merged.files_read)?,
                serde_json::to_string(&merged.files_modified)?,
                merged.narrative,
                merged.created_at.with_timezone(&Utc).to_rfc3339(),
                serde_json::to_string(&merged.concepts)?,
                merged.noise_level.as_str(),
                merged.subtitle,
                existing_id,
            ],
        )?;

        tx.commit()?;
        Ok(())
    }
}
