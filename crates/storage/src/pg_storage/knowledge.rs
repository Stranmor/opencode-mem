//! KnowledgeStore implementation for PgStorage.

use super::*;

use crate::error::StorageError;
use crate::traits::KnowledgeStore;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use opencode_mem_core::{
    GlobalKnowledge, KNOWLEDGE_TRIGRAM_CANDIDATE_LIMIT, KNOWLEDGE_TRIGRAM_LOG_THRESHOLD,
    KNOWLEDGE_TRIGRAM_MERGE_THRESHOLD, KnowledgeInput, KnowledgeSearchResult, KnowledgeType,
};
use sqlx::Row;

type ExistingKnowledgeRow = (
    String,
    DateTime<Utc>,
    serde_json::Value,
    serde_json::Value,
    serde_json::Value,
    f64,
    i64,
    Option<DateTime<Utc>>,
);

impl PgStorage {
    fn merge_provenance(existing: &mut Vec<String>, new_value: Option<&String>) {
        if let Some(val) = new_value
            && !existing.contains(val)
        {
            existing.push(val.clone());
        }
    }

    fn merge_triggers(existing: &mut Vec<String>, new_triggers: &[String]) {
        for t in new_triggers {
            if !existing.contains(t) {
                existing.push(t.clone());
            }
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors ExistingKnowledgeRow tuple fields for merge operation"
    )]
    async fn merge_into_existing(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        existing_id: &str,
        existing_created_at: DateTime<Utc>,
        existing_triggers: serde_json::Value,
        existing_src_proj: serde_json::Value,
        existing_src_obs: serde_json::Value,
        existing_confidence: f64,
        existing_usage_count: i64,
        existing_last_used_at: Option<DateTime<Utc>>,
        existing_title: &str,
        input: &KnowledgeInput,
        now: DateTime<Utc>,
    ) -> Result<GlobalKnowledge, StorageError> {
        let mut triggers: Vec<String> = parse_json_value(existing_triggers, "triggers")?;
        let mut source_projects: Vec<String> =
            parse_json_value(existing_src_proj, "source_projects")?;
        let mut source_observations: Vec<String> =
            parse_json_value(existing_src_obs, "source_observations")?;

        Self::merge_triggers(&mut triggers, &input.triggers);
        Self::merge_provenance(&mut source_projects, input.source_project.as_ref());
        Self::merge_provenance(&mut source_observations, input.source_observation.as_ref());

        sqlx::query(
            "UPDATE global_knowledge
             SET knowledge_type = $1,
                 description = COALESCE(NULLIF($2, ''), description),
                 instructions = COALESCE(NULLIF($3, ''), instructions),
                 triggers = $4, source_projects = $5, source_observations = $6,
                 updated_at = $7, archived_at = NULL
             WHERE id = $8",
        )
        .bind(input.knowledge_type.as_str())
        .bind(&input.description)
        .bind(&input.instructions)
        .bind(serde_json::to_value(&triggers)?)
        .bind(serde_json::to_value(&source_projects)?)
        .bind(serde_json::to_value(&source_observations)?)
        .bind(now)
        .bind(existing_id)
        .execute(&mut **tx)
        .await?;

        // Re-read description/instructions from DB to capture COALESCE results
        let updated =
            sqlx::query("SELECT description, instructions FROM global_knowledge WHERE id = $1")
                .bind(existing_id)
                .fetch_one(&mut **tx)
                .await?;
        let merged_description: String = updated.get("description");
        let merged_instructions: Option<String> = updated.get("instructions");

        Ok(GlobalKnowledge::new(
            existing_id.to_owned(),
            input.knowledge_type,
            existing_title.to_owned(),
            merged_description,
            merged_instructions,
            triggers,
            source_projects,
            source_observations,
            existing_confidence,
            existing_usage_count,
            existing_last_used_at.map(|d| d.to_rfc3339()),
            existing_created_at.to_rfc3339(),
            now.to_rfc3339(),
            None,
        ))
    }

    async fn save_knowledge_inner(
        &self,
        id: Option<&str>,
        input: &KnowledgeInput,
    ) -> Result<GlobalKnowledge, StorageError> {
        let mut tx = self.pool.begin().await?;

        sqlx::query("SELECT pg_advisory_xact_lock(84572910)")
            .execute(&mut *tx)
            .await?;

        let now = Utc::now();

        let trimmed_title = input.title.trim();

        let existing: Option<ExistingKnowledgeRow> = sqlx::query_as(
            "SELECT id, created_at, triggers, source_projects, source_observations,
                        confidence, usage_count, last_used_at
                 FROM global_knowledge
                 WHERE LOWER(title) = LOWER($1)
                 FOR UPDATE",
        )
        .bind(trimmed_title)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some((
            existing_id,
            created_at,
            triggers_json,
            src_proj_json,
            src_obs_json,
            confidence,
            usage_count,
            last_used_at,
        )) = existing
        {
            let result = self
                .merge_into_existing(
                    &mut tx,
                    &existing_id,
                    created_at,
                    triggers_json,
                    src_proj_json,
                    src_obs_json,
                    confidence,
                    usage_count,
                    last_used_at,
                    trimmed_title,
                    input,
                    now,
                )
                .await?;
            tx.commit().await?;
            return Ok(result);
        }

        if let Some(similar) = self
            .find_trigram_similar_in_tx(&mut tx, trimmed_title)
            .await?
        {
            let result = self
                .merge_into_existing(
                    &mut tx, &similar.0, similar.1, similar.2, similar.3, similar.4, similar.5,
                    similar.6, similar.7, &similar.8, input, now,
                )
                .await?;
            tx.commit().await?;

            tracing::info!(
                new_title = trimmed_title,
                merged_into = %result.id,
                existing_title = %result.title,
                "knowledge trigram dedup: merged similar entry"
            );
            return Ok(result);
        }

        let id = id.map_or_else(|| uuid::Uuid::new_v4().to_string(), ToOwned::to_owned);
        let source_projects: Vec<String> = input
            .source_project
            .as_ref()
            .map(|p| vec![p.clone()])
            .unwrap_or_default();
        let source_observations: Vec<String> = input
            .source_observation
            .as_ref()
            .map(|o| vec![o.clone()])
            .unwrap_or_default();

        sqlx::query(&format!(
            "INSERT INTO global_knowledge ({KNOWLEDGE_COLUMNS})
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)"
        ))
        .bind(&id)
        .bind(input.knowledge_type.as_str())
        .bind(trimmed_title)
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
        .bind(Option::<DateTime<Utc>>::None)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(GlobalKnowledge::new(
            id,
            input.knowledge_type,
            trimmed_title.to_owned(),
            input.description.clone(),
            input.instructions.clone(),
            input.triggers.clone(),
            source_projects,
            source_observations,
            0.5,
            0,
            None,
            now.to_rfc3339(),
            now.to_rfc3339(),
            None,
        ))
    }

    #[expect(
        clippy::type_complexity,
        reason = "tuple matches ExistingKnowledgeRow + title + similarity"
    )]
    async fn find_trigram_similar_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        title: &str,
    ) -> Result<
        Option<(
            String,
            DateTime<Utc>,
            serde_json::Value,
            serde_json::Value,
            serde_json::Value,
            f64,
            i64,
            Option<DateTime<Utc>>,
            String,
        )>,
        StorageError,
    > {
        let rows: Vec<(
            String,
            DateTime<Utc>,
            serde_json::Value,
            serde_json::Value,
            serde_json::Value,
            f64,
            i64,
            Option<DateTime<Utc>>,
            String,
            f32,
        )> = sqlx::query_as(
            "SELECT id, created_at, triggers, source_projects, source_observations,
                    confidence, usage_count, last_used_at, title,
                    similarity(LOWER(title), LOWER($1)) as sim
             FROM global_knowledge
             WHERE archived_at IS NULL
               AND similarity(LOWER(title), LOWER($1)) > $2
             ORDER BY sim DESC
             LIMIT $3
             FOR UPDATE",
        )
        .bind(title)
        .bind(KNOWLEDGE_TRIGRAM_LOG_THRESHOLD)
        .bind(KNOWLEDGE_TRIGRAM_CANDIDATE_LIMIT)
        .fetch_all(&mut **tx)
        .await?;

        let mut best_merge: Option<(
            String,
            DateTime<Utc>,
            serde_json::Value,
            serde_json::Value,
            serde_json::Value,
            f64,
            i64,
            Option<DateTime<Utc>>,
            String,
        )> = None;

        for (
            id,
            created_at,
            triggers,
            src_proj,
            src_obs,
            confidence,
            usage_count,
            last_used_at,
            existing_title,
            sim,
        ) in rows
        {
            if sim >= KNOWLEDGE_TRIGRAM_MERGE_THRESHOLD {
                if best_merge.is_none() {
                    best_merge = Some((
                        id.clone(),
                        created_at,
                        triggers,
                        src_proj,
                        src_obs,
                        confidence,
                        usage_count,
                        last_used_at,
                        existing_title.clone(),
                    ));
                }
                tracing::debug!(
                    new_title = title,
                    existing_title = %existing_title,
                    similarity = %sim,
                    "knowledge trigram match above merge threshold"
                );
            } else {
                tracing::info!(
                    new_title = title,
                    existing_title = %existing_title,
                    existing_id = %id,
                    similarity = %sim,
                    "similar knowledge exists but below merge threshold"
                );
            }
        }

        Ok(best_merge)
    }
}

#[async_trait]
impl KnowledgeStore for PgStorage {
    async fn save_knowledge(&self, input: KnowledgeInput) -> Result<GlobalKnowledge, StorageError> {
        self.save_knowledge_with_id(&uuid::Uuid::new_v4().to_string(), input)
            .await
    }

    async fn save_knowledge_with_id(
        &self,
        id: &str,
        input: KnowledgeInput,
    ) -> Result<GlobalKnowledge, StorageError> {
        for attempt in 0u8..3u8 {
            match self.save_knowledge_inner(Some(id), &input).await {
                Ok(knowledge) => return Ok(knowledge),
                Err(ref e) if e.is_duplicate() && attempt < 2 => {
                    tracing::debug!(
                        title = %input.title,
                        attempt,
                        "knowledge save hit unique constraint, retrying"
                    );
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn get_knowledge(&self, id: &str) -> Result<Option<GlobalKnowledge>, StorageError> {
        let row = sqlx::query(&format!(
            "SELECT {KNOWLEDGE_COLUMNS} FROM global_knowledge WHERE id = $1 AND archived_at IS NULL"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| row_to_knowledge(&r)).transpose()
    }

    async fn delete_knowledge(&self, id: &str) -> Result<bool, StorageError> {
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
    ) -> Result<Vec<KnowledgeSearchResult>, StorageError> {
        let Some(tsquery) = build_tsquery(query) else {
            return self.list_knowledge(None, limit).await.map(|items| {
                items
                    .into_iter()
                    .map(|k| KnowledgeSearchResult::new(k, 1.0))
                    .collect()
            });
        };
        let rows = sqlx::query(&format!(
            "SELECT {KNOWLEDGE_COLUMNS},
                    ts_rank_cd(search_vec, to_tsquery('simple', $1))::float8 as score
             FROM global_knowledge
             WHERE search_vec @@ to_tsquery('simple', $1)
               AND archived_at IS NULL
             ORDER BY score DESC
             LIMIT $2"
        ))
        .bind(&tsquery)
        .bind(usize_to_i64(limit))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .filter_map(|r| {
                let score: f64 = match r.try_get("score") {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("Skipping knowledge row: score parse error: {e}");
                        return None;
                    }
                };
                match row_to_knowledge(r) {
                    Ok(k) => Some(KnowledgeSearchResult::new(k, score)),
                    Err(e) => {
                        tracing::warn!("Skipping corrupt knowledge row: {e}");
                        None
                    }
                }
            })
            .collect())
    }

    async fn list_knowledge(
        &self,
        knowledge_type: Option<KnowledgeType>,
        limit: usize,
    ) -> Result<Vec<GlobalKnowledge>, StorageError> {
        let rows = if let Some(kt) = knowledge_type {
            sqlx::query(&format!(
                "SELECT {KNOWLEDGE_COLUMNS} FROM global_knowledge
                 WHERE knowledge_type = $1 AND archived_at IS NULL
                 ORDER BY confidence DESC, usage_count DESC LIMIT $2"
            ))
            .bind(kt.as_str())
            .bind(usize_to_i64(limit))
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(&format!(
                "SELECT {KNOWLEDGE_COLUMNS} FROM global_knowledge
                 WHERE archived_at IS NULL
                 ORDER BY confidence DESC, usage_count DESC LIMIT $1"
            ))
            .bind(usize_to_i64(limit))
            .fetch_all(&self.pool)
            .await?
        };
        Ok(collect_skipping_corrupt(rows.iter().map(row_to_knowledge))?)
    }

    async fn update_knowledge_usage(&self, id: &str) -> Result<(), StorageError> {
        self.update_knowledge_usage_batch(&[id.to_owned()]).await
    }

    async fn update_knowledge_usage_batch(&self, ids: &[String]) -> Result<(), StorageError> {
        if ids.is_empty() {
            return Ok(());
        }
        let now = Utc::now();
        sqlx::query(
            "UPDATE global_knowledge \
             SET usage_count = usage_count + 1, \
                 last_used_at = $1, updated_at = $1, \
                 confidence = LEAST(1.0, confidence + 0.1) \
             WHERE id = ANY($2) AND archived_at IS NULL",
        )
        .bind(now)
        .bind(ids)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn has_knowledge_for_observation(
        &self,
        observation_id: &str,
    ) -> Result<bool, StorageError> {
        let json_array = serde_json::json!([observation_id]);
        let row: Option<(i32,)> = sqlx::query_as(
            "SELECT 1 FROM global_knowledge
             WHERE source_observations @> $1::jsonb
               AND archived_at IS NULL
             LIMIT 1",
        )
        .bind(&json_array)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.is_some())
    }

    async fn decay_confidence(&self) -> Result<u64, StorageError> {
        // Incremental decay: subtract 0.05 per week elapsed since last decay/usage.
        // Uses updated_at as reference — set to NOW() on every decay run AND on every
        // usage bump (record_knowledge_usage). This ensures each cron invocation only
        // decays by the time elapsed since the previous run, not cumulative from creation.
        // last_used_at is NOT modified — it retains its semantic meaning ("last retrieval").
        let result = sqlx::query(
            "UPDATE global_knowledge
             SET confidence = GREATEST(0.1,
                 confidence - 0.05 * EXTRACT(EPOCH FROM (NOW() - updated_at)) / 604800.0
             ),
             updated_at = NOW()
             WHERE archived_at IS NULL
               AND confidence > 0.1
               AND EXTRACT(EPOCH FROM (NOW() - updated_at)) > 604800.0",
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    async fn auto_archive(&self, min_age_days: i64) -> Result<u64, StorageError> {
        let min_age_days = min_age_days.max(0);
        let result = sqlx::query(
            "UPDATE global_knowledge
             SET archived_at = NOW(), updated_at = NOW()
             WHERE archived_at IS NULL
               AND confidence < 0.2
               AND usage_count = 0
               AND created_at < NOW() - ($1 || ' days')::INTERVAL",
        )
        .bind(min_age_days.to_string())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}
