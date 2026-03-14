//! LLM compression pipeline and candidate retrieval for context-aware observation creation.

use std::collections::HashSet;
use std::sync::Arc;

use opencode_mem_core::{
    Observation, ObservationInput, ToolCall, ToolOutput, is_trivial_tool_call, sanitize_input,
};
use opencode_mem_embeddings::EmbeddingProvider;
use opencode_mem_llm::CompressionResult;
use opencode_mem_storage::traits::{ObservationStore, SearchStore};

use super::ObservationService;
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

        let parsed_project = tool_call
            .project
            .as_deref()
            .filter(|p| !p.is_empty() && *p != "unknown");
        let input_text = serde_json::to_string(&tool_call.input).unwrap_or_default();
        let candidates = self
            .find_candidate_observations(
                &input_text,
                &filtered_output,
                tool_call.session_id.as_ref(),
                parsed_project,
            )
            .await;

        let compression_result = self
            .llm
            .compress_to_observation(id, &input, parsed_project, &candidates)
            .await?;

        match compression_result {
            CompressionResult::Skip { reason } => {
                tracing::debug!(reason = %reason, "Observation skipped by LLM");
                Ok(None)
            }
            CompressionResult::Create(mut observation) => {
                observation.title = sanitize_input(&observation.title);
                self.persist_and_notify(&observation, Some(tool_call.session_id.as_ref()))
                    .await
            }
            CompressionResult::Update {
                target_id,
                mut observation,
            } => {
                observation.title = sanitize_input(&observation.title);
                let candidate_ids: HashSet<&str> =
                    candidates.iter().map(|o| o.id.as_ref()).collect();

                if !candidate_ids.contains(target_id.as_str()) {
                    tracing::warn!(
                        target_id = %target_id,
                        "Update target not in candidate set — treating as create"
                    );
                    return self
                        .persist_and_notify(&observation, Some(tool_call.session_id.as_ref()))
                        .await;
                }

                match self
                    .storage
                    .merge_into_existing(&target_id, &observation, true)
                    .await
                {
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
                                self.persist_and_notify(
                                    &observation,
                                    Some(tool_call.session_id.as_ref()),
                                )
                                .await
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            target_id = %target_id,
                            error = %e,
                            "Merge failed, falling back to create"
                        );
                        self.persist_and_notify(&observation, Some(tool_call.session_id.as_ref()))
                            .await
                    }
                }
            }
        }
    }

    /// Build a search query from tool input + output, taking head+tail to capture
    /// both the beginning context and the final results of the tool call.
    fn build_candidate_query(tool_input: &str, tool_output: &str, max_len: usize) -> String {
        let half = max_len / 2;
        let mut parts = Vec::new();

        // Include tool input (file paths, function names, queries — high signal)
        if !tool_input.is_empty() {
            let input_budget = half.min(tool_input.len());
            let end = Self::find_char_boundary(tool_input, input_budget);
            if let Some(s) = tool_input.get(..end)
                && !s.is_empty()
            {
                parts.push(s.to_owned());
            }
        }

        // Include head + tail of tool output
        if !tool_output.is_empty() {
            let output_budget =
                max_len.saturating_sub(parts.iter().map(String::len).sum::<usize>());
            if tool_output.len() <= output_budget {
                parts.push(tool_output.to_owned());
            } else {
                let head_size = output_budget / 2;
                let tail_size = output_budget.saturating_sub(head_size);
                let head_end = Self::find_char_boundary(tool_output, head_size);
                if let Some(s) = tool_output.get(..head_end)
                    && !s.is_empty()
                {
                    parts.push(s.to_owned());
                }
                let tail_start_raw = tool_output.len().saturating_sub(tail_size);
                let tail_start = Self::find_char_boundary_up(tool_output, tail_start_raw);
                if let Some(s) = tool_output.get(tail_start..)
                    && !s.is_empty()
                {
                    parts.push(s.to_owned());
                }
            }
        }

        parts.join(" ")
    }

    /// Find the nearest char boundary at or below `pos`.
    fn find_char_boundary(s: &str, pos: usize) -> usize {
        let mut boundary = pos.min(s.len());
        while boundary > 0 && !s.is_char_boundary(boundary) {
            boundary = boundary.saturating_sub(1);
        }
        boundary
    }

    /// Find the nearest char boundary at or above `pos`.
    fn find_char_boundary_up(s: &str, pos: usize) -> usize {
        let mut boundary = pos.min(s.len());
        while boundary < s.len() && !s.is_char_boundary(boundary) {
            boundary = boundary.saturating_add(1);
        }
        boundary
    }

    async fn find_candidate_observations(
        &self,
        tool_input: &str,
        tool_output: &str,
        session_id: &str,
        project: Option<&str>,
    ) -> Vec<Observation> {
        let query_str = Self::build_candidate_query(tool_input, tool_output, 500);
        let mut hybrid_candidates = Vec::new();

        if !query_str.is_empty()
            && let Some(ref embeddings) = self.embeddings
        {
            let embed_input = query_str.clone();
            let embeddings_clone = Arc::clone(embeddings);
            let vector_res =
                tokio::task::spawn_blocking(move || embeddings_clone.embed(&embed_input)).await;

            match vector_res {
                Ok(Ok(query_vec)) => {
                    match self
                        .storage
                        .hybrid_search_v2_with_filters(
                            &query_str, &query_vec, project, None, None, None, 5,
                        )
                        .await
                    {
                        Ok(results) => {
                            let ids: Vec<String> =
                                results.into_iter().map(|r| String::from(r.id)).collect();
                            if !ids.is_empty() {
                                match self.storage.get_observations_by_ids(&ids).await {
                                    Ok(obs) => hybrid_candidates = obs,
                                    Err(e) => {
                                        tracing::warn!(error = %e, "Failed to fetch hybrid candidate observations by ids")
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Hybrid search for candidates failed")
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "Failed to generate embedding for candidates")
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Spawn blocking failed for candidate embedding generation")
                }
            }
        }

        let fts_query = Self::build_candidate_query(tool_input, tool_output, 500);
        let fts_candidates = self.find_fts_candidates(&fts_query, project).await;

        let session_candidates = match self
            .storage
            .get_recent_session_observations(session_id, 10)
            .await
        {
            Ok(obs) => obs,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to get session observations for candidates");
                Vec::new()
            }
        };

        let mut seen_ids: HashSet<String> = HashSet::new();
        let mut result: Vec<Observation> = Vec::new();

        for obs in hybrid_candidates
            .into_iter()
            .chain(fts_candidates)
            .chain(session_candidates)
        {
            if seen_ids.insert(String::from(obs.id.clone())) {
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

    async fn find_fts_candidates(&self, query: &str, project: Option<&str>) -> Vec<Observation> {
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
            }
        };

        if search_results.is_empty() {
            return Vec::new();
        }

        let ids: Vec<String> = search_results
            .into_iter()
            .map(|r| String::from(r.id))
            .collect();
        match self.storage.get_observations_by_ids(&ids).await {
            Ok(obs) => obs,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch candidate observations by ids");
                Vec::new()
            }
        }
    }
}
