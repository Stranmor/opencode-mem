use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use futures_util::stream::Stream;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;

use opencode_mem_core::{Observation, SearchResult};
use opencode_mem_storage::StorageStats;

use crate::api_types::{
    ContextPreview, ContextPreviewQuery, ContextQuery, SearchHelpResponse, SearchQuery,
    TimelineResult, UnifiedTimelineQuery,
};
use crate::blocking::{blocking_json, blocking_result};
use crate::AppState;

use super::api_docs::get_search_help;
use super::search::unified_timeline;

pub async fn get_context_recent(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ContextQuery>,
) -> Result<Json<Vec<Observation>>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    let project = query.project.clone();
    let limit = query.limit;
    blocking_json(move || storage.get_context_for_project(&project, limit)).await
}

pub async fn get_projects(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    blocking_json(move || storage.get_all_projects()).await
}

pub async fn get_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<StorageStats>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    blocking_json(move || storage.get_stats()).await
}

pub async fn get_context_inject(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ContextQuery>,
) -> Result<Json<Vec<Observation>>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    let project = query.project.clone();
    let limit = query.limit;
    blocking_json(move || storage.get_context_for_project(&project, limit)).await
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
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let q = if query.q.is_empty() { None } else { Some(query.q.clone()) };
    let storage = Arc::clone(&state.storage);
    let project = query.project.clone();
    let limit = query.limit;
    blocking_json(move || {
        storage.search_with_filters(q.as_deref(), project.as_deref(), Some("decision"), limit)
    })
    .await
}

pub async fn get_changes(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let q = if query.q.is_empty() { None } else { Some(query.q.clone()) };
    let storage = Arc::clone(&state.storage);
    let project = query.project.clone();
    let limit = query.limit;
    blocking_json(move || {
        storage.search_with_filters(q.as_deref(), project.as_deref(), Some("change"), limit)
    })
    .await
}

pub async fn get_how_it_works(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let search_query = if query.q.is_empty() {
        "how-it-works".to_owned()
    } else {
        format!("{} how-it-works", query.q)
    };
    let storage = Arc::clone(&state.storage);
    let limit = query.limit;
    blocking_json(move || storage.hybrid_search(&search_query, limit)).await
}

pub async fn context_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UnifiedTimelineQuery>,
) -> Result<Json<TimelineResult>, StatusCode> {
    unified_timeline(State(state), Query(query)).await
}

pub async fn context_preview(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ContextPreviewQuery>,
) -> Result<Json<ContextPreview>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    let project = query.project.clone();
    let limit = query.limit;
    let observations =
        blocking_result(move || storage.get_context_for_project(&project, limit)).await?;
    let preview = if query.format == "full" {
        observations
            .iter()
            .map(|o| {
                format!(
                    "[{}] {}: {}",
                    o.observation_type.as_str(),
                    o.title,
                    o.subtitle.as_deref().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    } else {
        observations.iter().map(|o| format!("\u{2022} {}", o.title)).collect::<Vec<_>>().join("\n")
    };
    Ok(Json(ContextPreview {
        project: query.project,
        observation_count: observations.len(),
        preview,
    }))
}

pub async fn timeline_by_query(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UnifiedTimelineQuery>,
) -> Result<Json<TimelineResult>, StatusCode> {
    unified_timeline(State(state), Query(query)).await
}

pub async fn search_help() -> Json<SearchHelpResponse> {
    Json(get_search_help())
}
