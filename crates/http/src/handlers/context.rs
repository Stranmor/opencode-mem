use crate::api_error::{ApiError, DegradedExt, OrDegraded};
use axum::{
    Json,
    extract::{Query, State},
    response::sse::{Event, Sse},
};
use chrono::{Datelike, Utc};
use futures_util::stream::Stream;
use serde_json::json;
use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;

use opencode_mem_core::{GlobalKnowledge, Observation, SearchResult};
use opencode_mem_service::StorageStats;

use crate::AppState;
use crate::api_types::{
    ContextInjectResponse, ContextPreview, ContextPreviewQuery, ContextQuery, SearchHelpResponse,
    SearchQuery, TimelineResult, UnifiedTimelineQuery,
};

use super::api_docs::get_search_help;
use super::search::unified_timeline;

pub async fn get_context_recent(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ContextQuery>,
) -> Result<Json<ContextInjectResponse>, ApiError> {
    let degraded_fallback = ContextInjectResponse {
        project: query.project.clone(),
        observations: Vec::new(),
        knowledge: Vec::new(),
        formatted_context: String::new(),
    };
    let observations = state
        .search_service
        .get_context_for_project(&query.project, query.limit)
        .await
        .or_degraded(degraded_fallback)?;

    if let Some(ref session_id) = query.session_id {
        let ids: Vec<String> = observations.iter().map(|o| o.id.to_string()).collect();
        if !ids.is_empty()
            && let Err(e) = state
                .observation_service
                .save_injected_observations(session_id, &ids)
                .await
        {
            tracing::warn!("Failed to record injected observations: {}", e);
        }
    }

    let knowledge = fetch_relevant_knowledge(&state, &query.project, 10).await;
    let formatted_context = format_context_sections(&observations, &knowledge);

    Ok(Json(ContextInjectResponse {
        project: query.project,
        observations,
        knowledge,
        formatted_context,
    }))
}

async fn fetch_relevant_knowledge(
    state: &AppState,
    project: &str,
    limit: usize,
) -> Vec<GlobalKnowledge> {
    let all_knowledge = match state.knowledge_service.list_knowledge(None, 1000).await {
        Ok(items) => items,
        Err(e) => {
            tracing::warn!("Failed to fetch knowledge for context inject: {}", e);
            return Vec::new();
        }
    };

    let selected = select_relevant_knowledge(all_knowledge, project, limit);

    let ids: Vec<String> = selected.iter().map(|item| item.id.clone()).collect();
    let knowledge_service = state.knowledge_service.clone();
    tokio::spawn(async move {
        if let Err(e) = knowledge_service.update_knowledge_usage_batch(&ids).await {
            tracing::warn!("Failed to update knowledge usage for context inject: {}", e);
        }
    });

    selected
}

/// Selects knowledge entries using a multi-tier allocation strategy to prevent
/// popularity bias (rich-get-richer death spiral where 93%+ entries never get retrieved).
///
/// Tier allocation for `limit=10`:
/// - Tier 1 (40%): Project-relevant entries sorted by confidence
/// - Tier 2 (20%): Recent entries (recency boost for new knowledge)
/// - Tier 3 (20%): Proven entries with highest usage_count
/// - Tier 4 (remaining): Exploration — never-seen entries rotated daily
#[expect(
    clippy::indexing_slicing,
    reason = "all indices come from enumerate() over the same entries slice"
)]
fn select_relevant_knowledge(
    mut entries: Vec<GlobalKnowledge>,
    project: &str,
    limit: usize,
) -> Vec<GlobalKnowledge> {
    if entries.is_empty() || limit == 0 {
        return Vec::new();
    }

    let normalized_project = project.trim().to_ascii_lowercase();
    let mut selected_indices: Vec<usize> = Vec::with_capacity(limit);
    let mut used: HashSet<usize> = HashSet::new();

    let remaining = |selected: &[usize]| limit.saturating_sub(selected.len());

    // Tier 1: Project-relevant entries (up to 40% of slots)
    let project_slots = (limit * 2 / 5).max(1);
    let tier1_take = remaining(&selected_indices).min(project_slots);
    let mut project_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, k)| {
            k.source_projects
                .iter()
                .any(|p| p.trim().to_ascii_lowercase() == normalized_project)
        })
        .map(|(i, _)| i)
        .collect();
    project_indices.sort_by(|&a, &b| entries[b].confidence.total_cmp(&entries[a].confidence));
    for idx in project_indices.into_iter().take(tier1_take) {
        used.insert(idx);
        selected_indices.push(idx);
    }

    if remaining(&selected_indices) == 0 {
        return extract_by_indices(&mut entries, selected_indices);
    }

    // Tier 2: Recent entries (up to 20% of slots)
    let recency_slots = (limit / 5).max(1);
    let tier2_take = remaining(&selected_indices).min(recency_slots);
    let mut recent_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(i, _)| !used.contains(i))
        .map(|(i, _)| i)
        .collect();
    recent_indices.sort_by(|&a, &b| entries[b].created_at.cmp(&entries[a].created_at));
    for idx in recent_indices.into_iter().take(tier2_take) {
        used.insert(idx);
        selected_indices.push(idx);
    }

    if remaining(&selected_indices) == 0 {
        return extract_by_indices(&mut entries, selected_indices);
    }

    // Tier 3: High-value proven entries (up to 20% of slots)
    let proven_slots = (limit / 5).max(1);
    let tier3_take = remaining(&selected_indices).min(proven_slots);
    let mut proven_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(i, k)| !used.contains(i) && k.usage_count > 0)
        .map(|(i, _)| i)
        .collect();
    proven_indices.sort_by(|&a, &b| {
        entries[b]
            .usage_count
            .cmp(&entries[a].usage_count)
            .then_with(|| entries[b].confidence.total_cmp(&entries[a].confidence))
    });
    for idx in proven_indices.into_iter().take(tier3_take) {
        used.insert(idx);
        selected_indices.push(idx);
    }

    if remaining(&selected_indices) == 0 {
        return extract_by_indices(&mut entries, selected_indices);
    }

    // Tier 4: Exploration — never-seen entries (remaining slots)
    let tier4_take = remaining(&selected_indices);
    let mut unseen_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(i, k)| !used.contains(i) && k.usage_count == 0)
        .map(|(i, _)| i)
        .collect();
    if !unseen_indices.is_empty() {
        let day_seed = u32::try_from(Utc::now().num_days_from_ce()).unwrap_or(0) as usize;
        let rotate_by = day_seed % unseen_indices.len();
        unseen_indices.rotate_left(rotate_by);
    }
    for idx in unseen_indices.into_iter().take(tier4_take) {
        used.insert(idx);
        selected_indices.push(idx);
    }

    if remaining(&selected_indices) == 0 {
        return extract_by_indices(&mut entries, selected_indices);
    }

    // Backfill from any unused entries
    let backfill_take = remaining(&selected_indices);
    for idx in (0..entries.len())
        .filter(|i| !used.contains(i))
        .take(backfill_take)
    {
        selected_indices.push(idx);
    }

    extract_by_indices(&mut entries, selected_indices)
}

fn extract_by_indices(
    entries: &mut Vec<GlobalKnowledge>,
    mut indices: Vec<usize>,
) -> Vec<GlobalKnowledge> {
    indices.sort_unstable_by(|a, b| b.cmp(a));
    let mut result = Vec::with_capacity(indices.len());
    for idx in indices {
        result.push(entries.swap_remove(idx));
    }
    result
}

fn format_context_sections(observations: &[Observation], knowledge: &[GlobalKnowledge]) -> String {
    let observations_block = if observations.is_empty() {
        "(none)".to_owned()
    } else {
        observations
            .iter()
            .map(|obs| {
                let base = format!("- [{}] {}", obs.observation_type.as_str(), obs.title,);
                match obs.subtitle.as_deref() {
                    Some(s) if !s.is_empty() => format!("{base} :: {s}"),
                    _ => base,
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let knowledge_block = if knowledge.is_empty() {
        "(none)".to_owned()
    } else {
        knowledge
            .iter()
            .map(|item| {
                format!(
                    "- [{}] {}\n  description: {}\n  instructions: {}\n  usage_count: {}",
                    item.knowledge_type.as_str(),
                    item.title,
                    item.description,
                    item.instructions.as_deref().unwrap_or("(none)"),
                    item.usage_count
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    format!(
        "=== RECENT OBSERVATIONS ===\n{}\n\n=== RELEVANT GLOBAL KNOWLEDGE ===\n{}",
        observations_block, knowledge_block
    )
}

#[cfg(test)]
mod tests {
    use super::select_relevant_knowledge;
    use opencode_mem_core::{GlobalKnowledge, KnowledgeType};

    fn sample_knowledge(
        id: &str,
        title: &str,
        source_projects: Vec<&str>,
        usage_count: i64,
    ) -> GlobalKnowledge {
        sample_knowledge_full(
            id,
            title,
            source_projects,
            usage_count,
            0.5,
            "2026-01-01T00:00:00Z",
        )
    }

    fn sample_knowledge_full(
        id: &str,
        title: &str,
        source_projects: Vec<&str>,
        usage_count: i64,
        confidence: f64,
        created_at: &str,
    ) -> GlobalKnowledge {
        GlobalKnowledge::new(
            id.to_owned(),
            KnowledgeType::Pattern,
            title.to_owned(),
            "description".to_owned(),
            None,
            vec![],
            source_projects.into_iter().map(str::to_owned).collect(),
            vec![],
            confidence,
            usage_count,
            None,
            created_at.to_owned(),
            created_at.to_owned(),
            None,
        )
    }

    #[test]
    fn select_relevant_knowledge_prioritizes_project_matches() {
        let entries = vec![
            sample_knowledge("global-100", "global high", vec![], 100),
            sample_knowledge("global-90", "global medium", vec![], 90),
            sample_knowledge("project-10", "project low", vec!["demo"], 10),
            sample_knowledge("project-1", "project tiny", vec!["demo"], 1),
        ];

        let selected = select_relevant_knowledge(entries, "demo", 3);

        assert_eq!(selected.len(), 3);
        // Tier 1 should include both project entries (limit*2/5 = 1 for limit=3, but we have 2 project entries)
        assert!(selected.iter().any(|k| k.id == "project-10"));
    }

    #[test]
    fn empty_input_returns_empty() {
        let selected = select_relevant_knowledge(vec![], "demo", 10);
        assert!(selected.is_empty());
    }

    #[test]
    fn zero_limit_returns_empty() {
        let entries = vec![sample_knowledge("a", "a", vec![], 0)];
        let selected = select_relevant_knowledge(entries, "demo", 0);
        assert!(selected.is_empty());
    }

    #[test]
    fn tier4_exploration_surfaces_unseen_entries() {
        // All entries have usage_count=0, none are project-specific
        let entries: Vec<_> = (0..20)
            .map(|i| sample_knowledge(&format!("k-{i}"), &format!("knowledge {i}"), vec![], 0))
            .collect();

        let selected = select_relevant_knowledge(entries, "demo", 10);

        assert_eq!(selected.len(), 10);
        // With daily rotation, different entries get surfaced
    }

    #[test]
    fn no_duplicate_ids_in_selection() {
        let entries = vec![
            sample_knowledge_full(
                "a",
                "proj entry",
                vec!["demo"],
                5,
                0.9,
                "2026-03-15T00:00:00Z",
            ),
            sample_knowledge_full("b", "recent", vec![], 0, 0.5, "2026-03-14T00:00:00Z"),
            sample_knowledge_full("c", "proven", vec![], 10, 0.7, "2026-01-01T00:00:00Z"),
            sample_knowledge_full("d", "unseen", vec![], 0, 0.3, "2026-01-01T00:00:00Z"),
        ];

        let selected = select_relevant_knowledge(entries, "demo", 4);

        let ids: Vec<_> = selected.iter().map(|k| k.id.as_str()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(
            ids.len(),
            unique.len(),
            "duplicate IDs in selection: {ids:?}"
        );
    }

    #[test]
    fn select_relevant_knowledge_obeys_limit() {
        let mut entries = Vec::new();
        // 5 project entries
        for i in 0..5 {
            entries.push(sample_knowledge_full(
                &format!("proj-{i}"),
                &format!("proj {i}"),
                vec!["demo"],
                0,
                0.9 - (i as f64 * 0.1),
                "2026-01-01T00:00:00Z",
            ));
        }
        // 5 proven entries
        for i in 0..5 {
            entries.push(sample_knowledge_full(
                &format!("proven-{i}"),
                &format!("proven {i}"),
                vec![],
                50 - (i as i64 * 10),
                0.8,
                "2026-01-01T00:00:00Z",
            ));
        }
        // 5 unseen entries
        for i in 0..5 {
            entries.push(sample_knowledge_full(
                &format!("unseen-{i}"),
                &format!("unseen {i}"),
                vec![],
                0,
                0.5,
                "2026-01-01T00:00:00Z",
            ));
        }

        // Test with various limits, especially small ones where the fractional rounding might exceed
        for limit in 1..=5 {
            let selected = select_relevant_knowledge(entries.clone(), "demo", limit);
            assert!(
                selected.len() <= limit,
                "Limit exceeded! requested: {limit}, returned: {}",
                selected.len()
            );
        }
    }

    #[test]
    fn multi_tier_allocation_with_limit_10() {
        let mut entries = Vec::new();
        // 3 project entries
        for i in 0..3 {
            entries.push(sample_knowledge_full(
                &format!("proj-{i}"),
                &format!("proj {i}"),
                vec!["demo"],
                0,
                0.9 - (i as f64 * 0.1),
                "2026-01-01T00:00:00Z",
            ));
        }
        // 3 proven entries (high usage)
        for i in 0..3 {
            entries.push(sample_knowledge_full(
                &format!("proven-{i}"),
                &format!("proven {i}"),
                vec![],
                50 - (i as i64 * 10),
                0.8,
                "2026-01-01T00:00:00Z",
            ));
        }
        // 10 unseen entries
        for i in 0..10 {
            entries.push(sample_knowledge_full(
                &format!("unseen-{i}"),
                &format!("unseen {i}"),
                vec![],
                0,
                0.5,
                "2026-01-01T00:00:00Z",
            ));
        }

        let selected = select_relevant_knowledge(entries, "demo", 10);

        assert_eq!(selected.len(), 10);

        let project_count = selected
            .iter()
            .filter(|k| k.id.starts_with("proj-"))
            .count();
        let proven_count = selected
            .iter()
            .filter(|k| k.id.starts_with("proven-"))
            .count();
        let unseen_count = selected
            .iter()
            .filter(|k| k.id.starts_with("unseen-"))
            .count();

        // Tier 1: 4 project slots, but only 3 available
        assert!(project_count >= 1, "should include project entries");
        // Tier 3: proven entries should appear
        assert!(proven_count >= 1, "should include proven entries");
        // Tier 4: unseen entries should fill remaining slots
        assert!(unseen_count >= 1, "should include exploration entries");
    }
}

pub async fn get_projects(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<String>>, ApiError> {
    state
        .search_service
        .get_all_projects()
        .await
        .or_degraded(Vec::<String>::new())
        .map(Json)
}

pub async fn get_stats(State(state): State<Arc<AppState>>) -> Result<Json<StorageStats>, ApiError> {
    state
        .search_service
        .get_stats()
        .await
        .or_degraded(StorageStats::default())
        .map(Json)
}

pub async fn sse_events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.event_tx.subscribe();
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(msg) => yield Ok(Event::default().data(msg)),
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!("SSE client lagged by {} messages", n);
                }
                Err(RecvError::Closed) => break,
            }
        }
    };
    Sse::new(stream)
}

pub async fn get_decisions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    let q = if query.q.is_empty() {
        None
    } else {
        Some(query.q.as_str())
    };
    state
        .search_service
        .search_with_filters(
            q,
            query.project.as_deref(),
            Some("decision"),
            None,
            None,
            query.capped_limit(),
        )
        .await
        .or_degraded(Vec::<SearchResult>::new())
        .map(Json)
}

pub async fn get_changes(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    let q = if query.q.is_empty() {
        None
    } else {
        Some(query.q.as_str())
    };
    state
        .search_service
        .search_with_filters(
            q,
            query.project.as_deref(),
            Some("change"),
            None,
            None,
            query.capped_limit(),
        )
        .await
        .or_degraded(Vec::<SearchResult>::new())
        .map(Json)
}

pub async fn get_how_it_works(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    let search_query = if query.q.is_empty() {
        "how-it-works".to_owned()
    } else {
        format!("{} how-it-works", query.q)
    };
    state
        .search_service
        .hybrid_search(&search_query, query.capped_limit())
        .await
        .or_degraded(Vec::<SearchResult>::new())
        .map(Json)
}

pub async fn context_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UnifiedTimelineQuery>,
) -> Result<Json<TimelineResult>, ApiError> {
    unified_timeline(State(state), Query(query)).await
}

pub async fn context_preview(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ContextPreviewQuery>,
) -> Result<Json<ContextPreview>, ApiError> {
    let observations = state
        .search_service
        .get_context_for_project(&query.project, query.limit)
        .await
        .or_degraded(Vec::<Observation>::new())?;

    let preview = if query.format == "full" {
        observations
            .iter()
            .map(|o| {
                let base = format!("[{}] {}", o.observation_type.as_str(), o.title,);
                match o.subtitle.as_deref() {
                    Some(s) if !s.is_empty() => format!("{base}: {s}"),
                    _ => base,
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    } else {
        observations
            .iter()
            .map(|o| format!("\u{2022} {}", o.title))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(Json(ContextPreview {
        project: query.project,
        observation_count: observations.len(),
        preview,
    }))
}

pub async fn search_help() -> Json<SearchHelpResponse> {
    Json(get_search_help())
}
