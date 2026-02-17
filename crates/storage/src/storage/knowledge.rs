use anyhow::Result;
use chrono::Utc;
use opencode_mem_core::{GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType};
use rusqlite::{params, OptionalExtension};

use super::knowledge_mapping::{row_to_knowledge, ExistingKnowledgeRow};
use super::{build_fts_query, get_conn, log_row_error, parse_json, Storage};

impl Storage {
    /// Save new knowledge entry.
    ///
    /// Retries on unique constraint violation (race condition where two
    /// concurrent transactions both find no existing row and both INSERT).
    ///
    /// # Errors
    /// Returns error if database insert fails after retries.
    pub fn save_knowledge(&self, input: KnowledgeInput) -> Result<GlobalKnowledge> {
        for attempt in 0u8..3u8 {
            match self.save_knowledge_inner(&input) {
                Ok(knowledge) => return Ok(knowledge),
                Err(e) => {
                    let is_constraint = e.downcast_ref::<rusqlite::Error>().is_some_and(|re| {
                        matches!(
                            re,
                            rusqlite::Error::SqliteFailure(
                                rusqlite::ffi::Error {
                                    code: rusqlite::ffi::ErrorCode::ConstraintViolation,
                                    ..
                                },
                                _,
                            )
                        )
                    });
                    if is_constraint && attempt < 2 {
                        tracing::debug!(
                            title = %input.title,
                            attempt,
                            "knowledge save hit unique constraint, retrying"
                        );
                        continue;
                    }
                    return Err(e);
                },
            }
        }
        unreachable!()
    }

    fn save_knowledge_inner(&self, input: &KnowledgeInput) -> Result<GlobalKnowledge> {
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
                        id: row.get("id")?,
                        created_at: row.get("created_at")?,
                        triggers: parse_json(&row.get::<_, String>("triggers")?)?,
                        source_projects: parse_json(&row.get::<_, String>("source_projects")?)?,
                        source_observations: parse_json(
                            &row.get::<_, String>("source_observations")?,
                        )?,
                        confidence: row.get("confidence")?,
                        usage_count: row.get("usage_count")?,
                        last_used_at: row.get("last_used_at")?,
                    })
                },
            )
            .optional()?;

        let now = Utc::now().to_rfc3339();

        if let Some(mut row) = existing {
            for t in &input.triggers {
                if !row.triggers.contains(t) {
                    row.triggers.push(t.clone());
                }
            }
            if let Some(ref p) = input.source_project {
                if !row.source_projects.contains(p) {
                    row.source_projects.push(p.clone());
                }
            }
            if let Some(ref o) = input.source_observation {
                if !row.source_observations.contains(o) {
                    row.source_observations.push(o.clone());
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
                input.knowledge_type.clone(),
                input.title.clone(),
                input.description.clone(),
                input.instructions.clone(),
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
                input.knowledge_type.clone(),
                input.title.clone(),
                input.description.clone(),
                input.instructions.clone(),
                input.triggers.clone(),
                input.source_project.clone().map(|p| vec![p]).unwrap_or_default(),
                input.source_observation.clone().map(|o| vec![o]).unwrap_or_default(),
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
            Ok(Some(row_to_knowledge(row)?))
        } else {
            Ok(None)
        }
    }

    /// Delete knowledge by ID.
    ///
    /// # Errors
    /// Returns error if database delete fails.
    pub fn delete_knowledge(&self, id: &str) -> Result<bool> {
        let conn = get_conn(&self.pool)?;
        let deleted = conn.execute("DELETE FROM global_knowledge WHERE id = ?1", params![id])?;
        Ok(deleted > 0)
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
        let fts_query = build_fts_query(query);

        if fts_query.is_empty() {
            return self.list_knowledge(None, limit).map(|items| {
                items.into_iter().map(|k| KnowledgeSearchResult::new(k, 1.0)).collect()
            });
        }

        let mut stmt = conn.prepare(
            "SELECT k.id, k.knowledge_type, k.title, k.description, k.instructions, k.triggers,
                      k.source_projects, k.source_observations, k.confidence, k.usage_count,
                      k.last_used_at, k.created_at, k.updated_at, bm25(global_knowledge_fts) as score
               FROM global_knowledge_fts
                JOIN global_knowledge k ON k.rowid = global_knowledge_fts.rowid
                WHERE global_knowledge_fts MATCH ?1
                ORDER BY score ASC
                LIMIT ?2",
        )?;

        let results = stmt
            .query_map(params![fts_query, limit], |row| {
                let score: f64 = row.get(13)?;
                Ok(KnowledgeSearchResult::new(row_to_knowledge(row)?, score.abs()))
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
                .query_map(params![kt.as_str(), limit], row_to_knowledge)?
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
                .query_map(params![limit], row_to_knowledge)?
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
}
