//! Direct memory storage — bypasses LLM compression pipeline.

use opencode_mem_core::{NoiseLevel, Observation, ObservationType, sanitize_input};
use opencode_mem_storage::traits::ObservationStore;

use super::{ObservationService, SaveMemoryResult};
use crate::ServiceError;

impl ObservationService {
    pub async fn save_memory(
        &self,
        text: &str,
        title: Option<&str>,
        project: Option<&str>,
        observation_type: Option<ObservationType>,
        noise_level: Option<NoiseLevel>,
    ) -> Result<SaveMemoryResult, ServiceError> {
        self.save_memory_with_id(
            &uuid::Uuid::new_v4().to_string(),
            text,
            title,
            project,
            observation_type,
            noise_level,
        )
        .await
    }

    pub async fn save_memory_with_id(
        &self,
        id: &str,
        text: &str,
        title: Option<&str>,
        project: Option<&str>,
        observation_type: Option<ObservationType>,
        noise_level: Option<NoiseLevel>,
    ) -> Result<SaveMemoryResult, ServiceError> {
        let text = sanitize_input(text.trim());
        if text.is_empty() {
            return Err(ServiceError::InvalidInput(
                "Text is required for save_memory".into(),
            ));
        }

        // Project filter check (Privacy)
        if let Some(p) = project
            && let Some(ref filter) = self.project_filter
            && filter.is_excluded(p)
        {
            tracing::info!(project = %p, "Skipping save_memory — project is excluded by privacy policy");
            return Ok(SaveMemoryResult::Filtered);
        }

        let title_str = match title {
            Some(t) if !t.trim().is_empty() => sanitize_input(t.trim()),
            _ => text.chars().take(50).collect(),
        };

        let project_str = project
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(ToOwned::to_owned);

        let resolved_observation_type = observation_type.unwrap_or(ObservationType::Discovery);
        let resolved_noise_level = noise_level.unwrap_or(NoiseLevel::Medium);

        let obs = Observation::builder(
            id.to_owned(),
            "manual".to_owned(),
            resolved_observation_type,
            title_str,
        )
        .maybe_project(project_str.clone().map(Into::into))
        .narrative(text.to_owned())
        .noise_level(resolved_noise_level)
        .build();

        if let Some(ref infinite_mem) = self.infinite_mem {
            let event = opencode_mem_core::tool_event(
                "manual",
                project_str.as_deref(),
                "save_memory",
                serde_json::json!({"text": text}),
                serde_json::json!({"output": "saved manually"}),
                vec![],
                None,
            );
            if let Err(e) = infinite_mem.store_event(event).await {
                tracing::warn!(error = %e, "Failed to store manual save_memory event in infinite memory");
            }
        }

        let result = self.persist_and_notify(&obs, None).await?;
        match result {
            Some((persisted_obs, _was_new)) => {
                self.spawn_enrichment(persisted_obs.clone());
                Ok(SaveMemoryResult::Created(persisted_obs))
            }
            None => Ok(SaveMemoryResult::Duplicate(obs)),
        }
    }

    fn spawn_enrichment(&self, obs: Observation) {
        let llm = self.llm.clone();
        let storage = self.storage.clone();
        let svc = self.clone();
        let semaphore = self.enrichment_semaphore.clone();

        tokio::spawn(async move {
            let _permit = match semaphore.acquire().await {
                Ok(permit) => permit,
                Err(_) => return, // Semaphore closed — shut down gracefully
            };

            let narrative = obs.narrative.as_deref().unwrap_or("");
            if narrative.is_empty() && obs.title.is_empty() {
                return;
            }

            match llm.enrich_observation_metadata(&obs.title, narrative).await {
                Ok(metadata) => {
                    let result = storage
                        .guarded(|| storage.update_observation_metadata(obs.id.as_ref(), &metadata))
                        .await;
                    if let Err(e) = result {
                        tracing::warn!(
                            observation_id = %obs.id,
                            error = %e,
                            "Failed to persist enriched metadata"
                        );
                    } else {
                        tracing::info!(
                            observation_id = %obs.id,
                            facts = metadata.facts.len(),
                            keywords = metadata.keywords.len(),
                            "Enriched save_memory observation with metadata"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        observation_id = %obs.id,
                        error = %e,
                        "LLM metadata enrichment failed"
                    );
                }
            }

            let fresh_obs = match storage.guarded(|| storage.get_by_id(obs.id.as_ref())).await {
                Ok(Some(refreshed)) => refreshed,
                Ok(None) => {
                    tracing::warn!(
                        observation_id = %obs.id,
                        "Observation disappeared after metadata update — skipping knowledge extraction"
                    );
                    return;
                }
                Err(e) => {
                    tracing::warn!(
                        observation_id = %obs.id,
                        error = %e,
                        "Failed to re-fetch observation after metadata update — using stale data"
                    );
                    obs
                }
            };

            if let Err(e) = svc.extract_knowledge(&fresh_obs).await {
                tracing::warn!(
                    observation_id = %fresh_obs.id,
                    error = %e,
                    "Knowledge extraction failed for save_memory observation"
                );
            }
        });
    }
}
