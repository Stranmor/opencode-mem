use std::io::{Error as IoError, ErrorKind};

use anyhow::Result;
use chrono::Utc;
use opencode_mem_core::{GlobalKnowledge, KnowledgeInput, KnowledgeSearchResult, KnowledgeType};
use rusqlite::params;

use super::{get_conn, log_row_error, parse_json, Storage};

impl Storage {
    /// Save new knowledge entry.
    ///
    /// # Errors
    /// Returns error if database insert fails.
    pub fn save_knowledge(&self, input: KnowledgeInput) -> Result<GlobalKnowledge> {
        let conn = get_conn(&self.pool)?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
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

        conn.execute(
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

        Ok(GlobalKnowledge {
            id,
            knowledge_type: input.knowledge_type,
            title: input.title,
            description: input.description,
            instructions: input.instructions,
            triggers: input.triggers,
            source_projects: input.source_project.map(|p| vec![p]).unwrap_or_default(),
            source_observations: input.source_observation.map(|o| vec![o]).unwrap_or_default(),
            confidence: 0.5,
            usage_count: 0,
            last_used_at: None,
            created_at: now.clone(),
            updated_at: now,
        })
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
                items
                    .into_iter()
                    .map(|k| KnowledgeSearchResult { knowledge: k, relevance_score: 1.0 })
                    .collect()
            });
        }

        let mut stmt = conn.prepare(
            "SELECT k.id, k.knowledge_type, k.title, k.description, k.instructions, k.triggers,
                      k.source_projects, k.source_observations, k.confidence, k.usage_count,
                      k.last_used_at, k.created_at, k.updated_at, bm25(global_knowledge_fts) as score
               FROM global_knowledge_fts f
               JOIN global_knowledge k ON k.rowid = f.rowid
               WHERE global_knowledge_fts MATCH ?1
               ORDER BY score
               LIMIT ?2",
        )?;

        let results = stmt
            .query_map(params![fts_query, limit], |row| {
                let score: f64 = row.get(13)?;
                Ok(KnowledgeSearchResult {
                    knowledge: Self::row_to_knowledge(row)?,
                    relevance_score: score.abs(),
                })
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

        Ok(GlobalKnowledge {
            id: row.get(0)?,
            knowledge_type,
            title: row.get(2)?,
            description: row.get(3)?,
            instructions: row.get(4)?,
            triggers: parse_json(&row.get::<_, String>(5)?)?,
            source_projects: parse_json(&row.get::<_, String>(6)?)?,
            source_observations: parse_json(&row.get::<_, String>(7)?)?,
            confidence: row.get(8)?,
            usage_count: row.get(9)?,
            last_used_at: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
        })
    }
}
