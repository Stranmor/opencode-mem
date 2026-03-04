//! LLM compression pipeline and candidate retrieval for context-aware observation creation.

use std::collections::HashSet;
use std::sync::Arc;

use opencode_mem_core::{
    Observation, ObservationInput, ObservationType, ToolCall, ToolOutput, is_trivial_tool_call,
    sanitize_input,
};
use opencode_mem_embeddings::EmbeddingProvider;
use opencode_mem_llm::CompressionResult;
use opencode_mem_storage::traits::{ObservationStore, SearchStore};

use super::{ObservationService, SaveMemoryResult};
use crate::ServiceError;

impl ObservationService {
    pub async fn compress_and_save(
        &self,
        id: &str,
        tool_call: &ToolCall,
    ) -> Result<Option<(Observation, bool)>, ServiceError> {
        if is_trivial_tool_call(&tool_call.tool, &tool_call.input) {
            tracing::debug!(tool = %tool_call.tool, "Bypassing LLM compression for trivial tool call");
            return Ok(None);
        }

        let filtered_output = sanitize_input(&tool_call.output);
        let filtered_input = {
            let input_str = serde_json::to_string(&tool_call.input).unwrap_or_default();
            let filtered = sanitize_input(&input_str);
            serde_json::from_str(&filtered).unwrap_or_else(|e| {
                tracing::warn!(
                    error = %e,
                    "Privacy/injection filter corrupted JSON input — using Null instead of unfiltered fallback"
                );
                serde_json::Value::Null
            })
        };

        let input = ObservationInput::new(
            tool_call.tool.clone(),
            tool_call.session_id.clone(),
            tool_call.call_id.clone(),
            ToolOutput::new(
                format!("Observation from {}", tool_call.tool),
                filtered_output.clone(),
                filtered_input,
            ),
        );

        let parsed_project =
            tool_call.project.as_deref().filter(|p| !p.is_empty() && *p != "unknown");
        let candidates = self
            .find_candidate_observations(&filtered_output, &tool_call.session_id, parsed_project)
            .await;

        let compression_result =
            self.llm.compress_to_observation(id, &input, parsed_project, &candidates).await?;

        match compression_result {
            CompressionResult::Skip { reason } => {
                tracing::debug!(reason = %reason, "Observation skipped by LLM");
                Ok(None)
            },
            CompressionResult::Create(mut observation) => {
                observation.title = sanitize_input(&observation.title);
                self.persist_and_notify(&observation, Some(&tool_call.session_id)).await
            },
            CompressionResult::Update { target_id, mut observation } => {
                observation.title = sanitize_input(&observation.title);
                let candidate_ids: HashSet<&str> =
                    candidates.iter().map(|o| o.id.as_str()).collect();
                if !candidate_ids.contains(target_id.as_str()) {
                    tracing::warn!(
                        target_id = %target_id,
                        "Update target not in candidate set at service layer — treating as create"
                    );
                    return self
                        .persist_and_notify(&observation, Some(&tool_call.session_id))
                        .await;
                }

                match self.storage.merge_into_existing(&target_id, &observation).await {
                    Ok(()) => {
                        tracing::info!(
                            target_id = %target_id,
                            title = %observation.title,
                            "Context-aware update: merged into existing observation"
                        );
                        self.regenerate_embedding(&target_id).await;
                        let merged_obs = self.storage.get_by_id(&target_id).await?;
                        match merged_obs {
                            Some(obs) => Ok(Some((obs, false))),
                            None => {
                                tracing::warn!(
                                    target_id = %target_id,
                                    "Merged observation disappeared after merge, saving as new"
                                );
                                self.persist_and_notify(&observation, Some(&tool_call.session_id))
                                    .await
                            },
                        }
                    },
                    Err(e) => {
                        tracing::warn!(
                            target_id = %target_id,
                            error = %e,
                            "Merge failed, falling back to create"
                        );
                        self.persist_and_notify(&observation, Some(&tool_call.session_id)).await
                    },
                }
            },
        }
    }

    async fn find_candidate_observations(
        &self,
        raw_text: &str,
        session_id: &str,
        project: Option<&str>,
    ) -> Vec<Observation> {
        let mut hybrid_candidates = Vec::new();
        if let Some(ref embeddings) = self.embeddings {
            let max_query_len = 500;
            let end = if raw_text.len() <= max_query_len {
                raw_text.len()
            } else {
                let mut boundary = max_query_len;
                while boundary > 0 && !raw_text.is_char_boundary(boundary) {
                    boundary = boundary.saturating_sub(1);
                }
                boundary
            };
            let query_str = raw_text.get(..end).unwrap_or("").to_owned();

            if !query_str.is_empty() {
                let embeddings_clone = Arc::clone(embeddings);
                let vector_res =
                    tokio::task::spawn_blocking(move || embeddings_clone.embed(&query_str)).await;

                match vector_res {
                    Ok(Ok(query_vec)) => {
                        match self
                            .storage
                            .hybrid_search_v2_with_filters(
                                raw_text.get(..end).unwrap_or(""),
                                &query_vec,
                                project,
                                None,
                                None,
                                None,
                                5,
                            )
                            .await
                        {
                            Ok(results) => {
                                let ids: Vec<String> = results.into_iter().map(|r| r.id).collect();
                                if !ids.is_empty() {
                                    match self.storage.get_observations_by_ids(&ids).await {
                                        Ok(obs) => hybrid_candidates = obs,
                                        Err(e) => {
                                            tracing::warn!(error = %e, "Failed to fetch hybrid candidate observations by ids")
                                        },
                                    }
                                }
                            },
                            Err(e) => {
                                tracing::warn!(error = %e, "Hybrid search for candidates failed")
                            },
                        }
                    },
                    Ok(Err(e)) => {
                        tracing::warn!(error = %e, "Failed to generate embedding for candidates")
                    },
                    Err(e) => {
                        tracing::warn!(error = %e, "Spawn blocking failed for candidate embedding generation")
                    },
                }
            }
        }

        let fts_candidates = self.find_fts_candidates(raw_text, project).await;

        let session_candidates =
            match self.storage.get_recent_session_observations(session_id, 3).await {
                Ok(obs) => obs,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to get session observations for candidates");
                    Vec::new()
                },
            };

        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut result: Vec<Observation> = Vec::new();

        for obs in hybrid_candidates.into_iter().chain(fts_candidates).chain(session_candidates) {
            if seen_ids.insert(obs.id.clone()) {
                result.push(obs);
            }
        }

        if !result.is_empty() {
            tracing::debug!(
                count = result.len(),
                "Found candidate observations for context-aware compression"
            );
        }

        result
    }

    async fn find_fts_candidates(&self, raw_text: &str, project: Option<&str>) -> Vec<Observation> {
        let max_query_len = 500;
        let end = if raw_text.len() <= max_query_len {
            raw_text.len()
        } else {
            let mut boundary = max_query_len;
            while boundary > 0 && !raw_text.is_char_boundary(boundary) {
                boundary = boundary.saturating_sub(1);
            }
            boundary
        };
        let query = raw_text.get(..end).unwrap_or("");
        if query.is_empty() {
            return Vec::new();
        }

        let search_results = match self
            .storage
            .hybrid_search_v2_with_filters(query, &[], project, None, None, None, 5)
            .await
        {
            Ok(results) => results,
            Err(e) => {
                tracing::warn!(error = %e, "FTS search for candidates failed");
                return Vec::new();
            },
        };

        if search_results.is_empty() {
            return Vec::new();
        }

        let ids: Vec<String> = search_results.into_iter().map(|r| r.id).collect();
        match self.storage.get_observations_by_ids(&ids).await {
            Ok(obs) => obs,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch candidate observations by ids");
                Vec::new()
            },
        }
    }

    pub async fn save_memory(
        &self,
        text: &str,
        title: Option<&str>,
        project: Option<&str>,
    ) -> Result<SaveMemoryResult, ServiceError> {
        let text = sanitize_input(text.trim());
        if text.is_empty() {
            return Err(ServiceError::InvalidInput("Text is required for save_memory".into()));
        }

        let title_str = match title {
            Some(t) if !t.trim().is_empty() => sanitize_input(t.trim()),
            _ => text.chars().take(50).collect(),
        };

        let project_str = project.map(str::trim).filter(|p| !p.is_empty()).map(ToOwned::to_owned);

        let obs = Observation::builder(
            uuid::Uuid::new_v4().to_string(),
            "manual".to_owned(),
            ObservationType::Discovery,
            title_str,
        )
        .maybe_project(project_str.clone())
        .narrative(text.to_owned())
        .build();

        if let Some(ref infinite_mem) = self.infinite_mem {
            let event = opencode_mem_infinite::tool_event(
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
            Some((persisted_obs, _was_new)) => Ok(SaveMemoryResult::Created(persisted_obs)),
            None => Ok(SaveMemoryResult::Duplicate(obs)),
        }
    }
}
