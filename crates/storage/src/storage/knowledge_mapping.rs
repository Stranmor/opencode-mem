use super::parse_json;
use opencode_mem_core::{GlobalKnowledge, KnowledgeType};
use std::io::{Error as IoError, ErrorKind};

/// Existing knowledge row data fetched from the database during upsert.
pub(super) struct ExistingKnowledgeRow {
    pub(super) id: String,
    pub(super) created_at: String,
    pub(super) triggers: Vec<String>,
    pub(super) source_projects: Vec<String>,
    pub(super) source_observations: Vec<String>,
    pub(super) confidence: f64,
    pub(super) usage_count: i64,
    pub(super) last_used_at: Option<String>,
}

pub(super) fn row_to_knowledge(row: &rusqlite::Row<'_>) -> rusqlite::Result<GlobalKnowledge> {
    let knowledge_type_str: String = row.get(1)?;
    let knowledge_type = knowledge_type_str.parse::<KnowledgeType>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(IoError::new(ErrorKind::InvalidData, e)),
        )
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
