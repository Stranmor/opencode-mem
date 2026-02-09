use std::io::{Error as IoError, ErrorKind};

use anyhow::Result;
use chrono::Utc;
use opencode_mem_core::{GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType};
use rusqlite::{params, OptionalExtension};

use super::{get_conn, log_row_error, parse_json, Storage};

/// Existing knowledge row data fetched from the database during upsert.
struct ExistingKnowledgeRow {
    id: String,
    created_at: String,
    triggers: Vec<String>,
    source_projects: Vec<String>,
    source_observations: Vec<String>,
    confidence: f64,
    usage_count: i64,
    last_used_at: Option<String>,
}

impl Storage {
    /// Save new knowledge entry.
    ///
    /// # Errors
    /// Returns error if database insert fails.
    pub fn save_knowledge(&self, input: KnowledgeInput) -> Result<GlobalKnowledge> {
        let mut conn = get_conn(&self.pool)?;
        let tx = conn.transaction()?;

        let existing: Option<ExistingKnowledgeRow> = tx
            .query_row(
                "SELECT id, created_at, triggers, source_projects, source_observations, confidence, usage_count, last_used_at 
                 FROM global_knowledge 
                 WHERE LOWER(TRIM(title)) = LOWER(TRIM(?1))",
                params![input.title],
                |row| {
                    Ok(ExistingKnowledgeRow {
                        id: row.get(0)?,
                        created_at: row.get(1)?,
                        triggers: parse_json(&row.get::<_, String>(2)?)?,
                        source_projects: parse_json(&row.get::<_, String>(3)?)?,
                        source_observations: parse_json(&row.get::<_, String>(4)?)?,
                        confidence: row.get(5)?,
                        usage_count: row.get(6)?,
                        last_used_at: row.get(7)?,
                    })
                },
            )
            .optional()?;

        let now = Utc::now().to_rfc3339();

        if let Some(mut row) = existing {
            for t in input.triggers {
                if !row.triggers.contains(&t) {
                    row.triggers.push(t);
                }
            }
            if let Some(p) = input.source_project {
                if !row.source_projects.contains(&p) {
                    row.source_projects.push(p);
                }
            }
            if let Some(o) = input.source_observation {
                if !row.source_observations.contains(&o) {
                    row.source_observations.push(o);
                }
            }

            tx.execute(
                "UPDATE global_knowledge 
                 SET knowledge_type = ?1,
                     description = ?2, 
                     instructions = ?3, 
                     triggers = ?4, 
                     source_projects = ?5, 
                     source_observations = ?6,
                     updated_at = ?7
                 WHERE id = ?8",
                params![
                    input.knowledge_type.as_str(),
                    input.description,
                    input.instructions,
                    serde_json::to_string(&row.triggers)?,
                    serde_json::to_string(&row.source_projects)?,
                    serde_json::to_string(&row.source_observations)?,
                    now,
                    row.id
                ],
            )?;

            tx.commit()?;

            Ok(GlobalKnowledge::new(
                row.id,
                input.knowledge_type,
                input.title,
                input.description,
                input.instructions,
                row.triggers,
                row.source_projects,
                row.source_observations,
                row.confidence,
                row.usage_count,
                row.last_used_at,
                row.created_at,
                now,
            ))
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            let triggers_json = serde_json::to_string(&input.triggers)?;
            let source_projects_json = input
                .source_project
                .as_ref()
                .map(|p| serde_json::to_string(&vec![p]))
                .transpose()?
                .unwrap_or_else(|| "[]".to_owned());
            let source_observations_json = input
                .source_observation
                .as_ref()
                .map(|o| serde_json::to_string(&vec![o]))
                .transpose()?
                .unwrap_or_else(|| "[]".to_owned());

            tx.execute(
                "INSERT INTO global_knowledge 
                   (id, knowledge_type, title, description, instructions, triggers, 
                    source_projects, source_observations, confidence, usage_count, 
                    last_used_at, created_at, updated_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    id,
                    input.knowledge_type.as_str(),
                    input.title,
                    input.description,
                    input.instructions,
                    triggers_json,
                    source_projects_json,
                    source_observations_json,
                    0.5f64,
                    0i64,
                    Option::<String>::None,
                    now,
                    now,
                ],
            )?;

            tx.commit()?;

            Ok(GlobalKnowledge::new(
                id,
                input.knowledge_type,
                input.title,
                input.description,
                input.instructions,
                input.triggers,
                input.source_project.map(|p| vec![p]).unwrap_or_default(),
                input.source_observation.map(|o| vec![o]).unwrap_or_default(),
                0.5,
                0,
                None,
                now.clone(),
                now,
            ))
        }
    }

    /// Get knowledge by ID.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>> {
        let conn = get_conn(&self.pool)?;
        let mut stmt = conn.prepare(
            "SELECT id, knowledge_type, title, description, instructions, triggers,
                      source_projects, source_observations, confidence, usage_count,
                      last_used_at, created_at, updated_at
               FROM global_knowledge WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Self::row_to_knowledge(row)?))
        } else {
            Ok(None)
        }
    }

    /// Search knowledge using FTS.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>> {
        let conn = get_conn(&self.pool)?;
        let fts_query = query
            .split_whitespace()
            .map(|word| format!("\"{}\"*", word.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" AND ");

        if fts_query.is_empty() {
            return self.list_knowledge(None, limit).map(|items| {
                items.into_iter().map(|k| KnowledgeSearchResult::new(k, 1.0)).collect()
            });
        }

        let mut stmt = conn.prepare(
            "SELECT k.id, k.knowledge_type, k.title, k.description, k.instructions, k.triggers,
                      k.source_projects, k.source_observations, k.confidence, k.usage_count,
                      k.last_used_at, k.created_at, k.updated_at, bm25(f) as score
               FROM global_knowledge_fts f
               JOIN global_knowledge k ON k.rowid = f.rowid
               WHERE f MATCH ?1
               ORDER BY score
               LIMIT ?2",
        )?;

        let results = stmt
            .query_map(params![fts_query, limit], |row| {
                let score: f64 = row.get(13)?;
                Ok(KnowledgeSearchResult::new(Self::row_to_knowledge(row)?, score.abs()))
            })?
            .filter_map(log_row_error)
            .collect();
        Ok(results)
    }

    /// List knowledge entries.
    ///
    /// # Errors
    /// Returns error if database query fails.
    pub fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>> {
        let conn = get_conn(&self.pool)?;
        if let Some(kt) = knowledge_type {
            let mut stmt = conn.prepare(
                "SELECT id, knowledge_type, title, description, instructions, triggers,
                          source_projects, source_observations, confidence, usage_count,
                          last_used_at, created_at, updated_at
                   FROM global_knowledge WHERE knowledge_type = ?1
                   ORDER BY confidence DESC, usage_count DESC
                   LIMIT ?2",
            )?;
            let results = stmt
                .query_map(params![kt.as_str(), limit], Self::row_to_knowledge)?
                .filter_map(log_row_error)
                .collect();
            Ok(results)
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, knowledge_type, title, description, instructions, triggers,
                          source_projects, source_observations, confidence, usage_count,
                          last_used_at, created_at, updated_at
                   FROM global_knowledge
                   ORDER BY confidence DESC, usage_count DESC
                   LIMIT ?1",
            )?;
            let results = stmt
                .query_map(params![limit], Self::row_to_knowledge)?
                .filter_map(log_row_error)
                .collect();
            Ok(results)
        }
    }

    /// Increment usage count and update confidence.
    ///
    /// # Errors
    /// Returns error if database update fails.
    pub fn update_knowledge_usage(&self, id: &str) -> Result<()> {
        let conn = get_conn(&self.pool)?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE global_knowledge 
               SET usage_count = usage_count + 1, 
                   last_used_at = ?1,
                   updated_at = ?1,
                   confidence = MIN(1.0, confidence + 0.05)
               WHERE id = ?2",
            params![now, id],
        )?;
        Ok(())
    }

    fn row_to_knowledge(row: &rusqlite::Row<'_>) -> rusqlite::Result<GlobalKnowledge> {
        let knowledge_type_str: String = row.get(1)?;
        let knowledge_type = knowledge_type_str.parse::<KnowledgeType>().map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(IoError::new(
                ErrorKind::InvalidData,
                e,
            )))
        })?;

        Ok(GlobalKnowledge::new(
            row.get(0)?,
            knowledge_type,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            parse_json(&row.get::<_, String>(5)?)?,
            parse_json(&row.get::<_, String>(6)?)?,
            parse_json(&row.get::<_, String>(7)?)?,
            row.get(8)?,
            row.get(9)?,
            row.get(10)?,
            row.get(11)?,
            row.get(12)?,
        ))
    }
}
