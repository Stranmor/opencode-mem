//! KnowledgeStore implementation for PgStorage.

use super::*;

use crate::traits::KnowledgeStore;
use async_trait::async_trait;
use opencode_mem_core::{KnowledgeInput, KnowledgeSearchResult};

#[async_trait]
impl KnowledgeStore for PgStorage {
    async fn save_knowledge(&self, input: KnowledgeInput) -> Result<GlobalKnowledge> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();

        let existing: Option<(
            String,
            DateTime<Utc>,
            serde_json::Value,
            serde_json::Value,
            serde_json::Value,
            f64,
            i32,
            Option<DateTime<Utc>>,
        )> = sqlx::query_as(
            "SELECT id, created_at, triggers, source_projects, source_observations,
                        confidence, usage_count, last_used_at
                 FROM global_knowledge
                 WHERE LOWER(TRIM(title)) = LOWER(TRIM($1))",
        )
        .bind(&input.title)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some((
            id,
            created_at,
            triggers_json,
            src_proj_json,
            src_obs_json,
            confidence,
            usage_count,
            last_used_at,
        )) = existing
        {
            let mut triggers: Vec<String> = parse_json_value(&triggers_json);
            let mut source_projects: Vec<String> = parse_json_value(&src_proj_json);
            let mut source_observations: Vec<String> = parse_json_value(&src_obs_json);

            for t in &input.triggers {
                if !triggers.contains(t) {
                    triggers.push(t.clone());
                }
            }
            if let Some(ref p) = input.source_project {
                if !source_projects.contains(p) {
                    source_projects.push(p.clone());
                }
            }
            if let Some(ref o) = input.source_observation {
                if !source_observations.contains(o) {
                    source_observations.push(o.clone());
                }
            }

            sqlx::query(
                "UPDATE global_knowledge
                 SET knowledge_type = $1, description = $2, instructions = $3,
                     triggers = $4, source_projects = $5, source_observations = $6,
                     updated_at = $7
                 WHERE id = $8",
            )
            .bind(input.knowledge_type.as_str())
            .bind(&input.description)
            .bind(&input.instructions)
            .bind(serde_json::to_value(&triggers)?)
            .bind(serde_json::to_value(&source_projects)?)
            .bind(serde_json::to_value(&source_observations)?)
            .bind(now)
            .bind(&id)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;

            Ok(GlobalKnowledge::new(
                id,
                input.knowledge_type,
                input.title,
                input.description,
                input.instructions,
                triggers,
                source_projects,
                source_observations,
                confidence,
                i64::from(usage_count),
                last_used_at.map(|d| d.to_rfc3339()),
                created_at.to_rfc3339(),
                now.to_rfc3339(),
            ))
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            let source_projects: Vec<String> =
                input.source_project.as_ref().map(|p| vec![p.clone()]).unwrap_or_default();
            let source_observations: Vec<String> =
                input.source_observation.as_ref().map(|o| vec![o.clone()]).unwrap_or_default();

            sqlx::query(&format!(
                "INSERT INTO global_knowledge ({KNOWLEDGE_COLUMNS})
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"
            ))
            .bind(&id)
            .bind(input.knowledge_type.as_str())
            .bind(&input.title)
            .bind(&input.description)
            .bind(&input.instructions)
            .bind(serde_json::to_value(&input.triggers)?)
            .bind(serde_json::to_value(&source_projects)?)
            .bind(serde_json::to_value(&source_observations)?)
            .bind(0.5f64)
            .bind(0i64)
            .bind(Option::<DateTime<Utc>>::None)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;

            Ok(GlobalKnowledge::new(
                id,
                input.knowledge_type,
                input.title,
                input.description,
                input.instructions,
                input.triggers,
                source_projects,
                source_observations,
                0.5,
                0,
                None,
                now.to_rfc3339(),
                now.to_rfc3339(),
            ))
        }
    }

    async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>> {
        let row =
            sqlx::query(&format!("SELECT {KNOWLEDGE_COLUMNS} FROM global_knowledge WHERE id = $1"))
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        row.map(|r| row_to_knowledge(&r)).transpose()
    }

    async fn delete_knowledge(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM global_knowledge WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeSearchResult>> {
        let tsquery = build_tsquery(query);
        if tsquery.is_empty() {
            return self.list_knowledge(None, limit).await.map(|items| {
                items.into_iter().map(|k| KnowledgeSearchResult::new(k, 1.0)).collect()
            });
        }
        let rows = sqlx::query(&format!(
            "SELECT {KNOWLEDGE_COLUMNS},
                    ts_rank_cd(search_vec, to_tsquery('english', $1))::float8 as score
             FROM global_knowledge
             WHERE search_vec @@ to_tsquery('english', $1)
             ORDER BY score DESC
             LIMIT $2"
        ))
        .bind(&tsquery)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                let score: f64 = r.try_get("score")?;
                Ok(KnowledgeSearchResult::new(row_to_knowledge(r)?, score))
            })
            .collect()
    }

    async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>> {
        let rows = if let Some(kt) = knowledge_type {
            sqlx::query(&format!(
                "SELECT {KNOWLEDGE_COLUMNS} FROM global_knowledge
                 WHERE knowledge_type = $1
                 ORDER BY confidence DESC, usage_count DESC LIMIT $2"
            ))
            .bind(kt.as_str())
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(&format!(
                "SELECT {KNOWLEDGE_COLUMNS} FROM global_knowledge
                 ORDER BY confidence DESC, usage_count DESC LIMIT $1"
            ))
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?
        };
        rows.iter().map(row_to_knowledge).collect()
    }

    async fn update_knowledge_usage(&self, id: &str) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            "UPDATE global_knowledge
             SET usage_count = usage_count + 1,
                 last_used_at = $1, updated_at = $1,
                 confidence = LEAST(1.0, confidence + 0.05)
             WHERE id = $2",
        )
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
