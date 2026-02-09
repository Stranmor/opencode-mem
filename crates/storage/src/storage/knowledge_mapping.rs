use super::parse_json;
use opencode_mem_core::{GlobalKnowledge, KnowledgeType};

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
    let knowledge_type_str: String = row.get("knowledge_type")?;
    let knowledge_type = knowledge_type_str.parse::<KnowledgeType>().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;

    Ok(GlobalKnowledge::new(
        row.get("id")?,
        knowledge_type,
        row.get("title")?,
        row.get("description")?,
        row.get("instructions")?,
        parse_json(&row.get::<_, String>("triggers")?)?,
        parse_json(&row.get::<_, String>("source_projects")?)?,
        parse_json(&row.get::<_, String>("source_observations")?)?,
        row.get("confidence")?,
        row.get("usage_count")?,
        row.get("last_used_at")?,
        row.get("created_at")?,
        row.get("updated_at")?,
    ))
}
