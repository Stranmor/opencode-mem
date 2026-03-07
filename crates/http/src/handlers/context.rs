use crate::api_error::{ApiError, DegradedExt, OrDegraded};
use axum::{
    Json,
    extract::{Query, State},
    response::sse::{Event, Sse},
};
use futures_util::stream::Stream;
use serde_json::json;
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
        for id in &ids {
            if let Err(e) = knowledge_service.update_knowledge_usage(id).await {
                tracing::warn!(
                    knowledge_id = %id,
                    "Failed to update knowledge usage for context inject: {}",
                    e
                );
            }
        }
    });

    selected
}

fn select_relevant_knowledge(
    mut entries: Vec<GlobalKnowledge>,
    project: &str,
    limit: usize,
) -> Vec<GlobalKnowledge> {
    if entries.is_empty() || limit == 0 {
        return Vec::new();
    }

    let normalized_project = project.trim().to_ascii_lowercase();
    entries.sort_by(|a, b| {
        b.usage_count
            .cmp(&a.usage_count)
            .then_with(|| b.confidence.total_cmp(&a.confidence))
            .then_with(|| a.title.cmp(&b.title))
    });

    let mut selected = Vec::with_capacity(limit);

    for entry in &entries {
        if entry.source_projects.iter().any(|source| {
            let normalized_source = source.trim().to_ascii_lowercase();
            normalized_source == normalized_project
        }) {
            selected.push(entry.clone());
            if selected.len() == limit {
                return selected;
            }
        }
    }

    for entry in entries {
        if selected.iter().any(|picked| picked.id == entry.id) {
            continue;
        }
        selected.push(entry);
        if selected.len() == limit {
            break;
        }
    }

    selected
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
        GlobalKnowledge::new(
            id.to_owned(),
            KnowledgeType::Pattern,
            title.to_owned(),
            "description".to_owned(),
            None,
            vec![],
            source_projects.into_iter().map(str::to_owned).collect(),
            vec![],
            0.5,
            usage_count,
            None,
            "2026-01-01T00:00:00Z".to_owned(),
            "2026-01-01T00:00:00Z".to_owned(),
            None,
        )
    }

    #[test]
    fn select_relevant_knowledge_prioritizes_project_matches_then_usage() {
        let entries = vec![
            sample_knowledge("global-100", "global high", vec![], 100),
            sample_knowledge("global-90", "global medium", vec![], 90),
            sample_knowledge("project-10", "project low", vec!["demo"], 10),
            sample_knowledge("project-1", "project tiny", vec!["demo"], 1),
        ];

        let selected = select_relevant_knowledge(entries, "demo", 3);

        assert_eq!(selected.len(), 3);
        assert_eq!(selected[0].id, "project-10");
        assert_eq!(selected[1].id, "project-1");
        assert_eq!(selected[2].id, "global-100");
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
