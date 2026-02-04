use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use futures_util::stream::Stream;
use std::convert::Infallible;
use std::sync::Arc;

use opencode_mem_core::{Observation, SearchResult};
use opencode_mem_storage::StorageStats;

use crate::api_types::{
    ContextPreview, ContextPreviewQuery, ContextQuery, SearchHelpResponse, SearchQuery,
    TimelineResult, UnifiedTimelineQuery,
};
use crate::AppState;

use super::api_docs::get_search_help;
use super::search::unified_timeline;

pub async fn get_context_recent(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ContextQuery>,
) -> Result<Json<Vec<Observation>>, StatusCode> {
    let storage = state.storage.clone();
    let project = query.project.clone();
    let limit = query.limit;
    tokio::task::spawn_blocking(move || storage.get_context_for_project(&project, limit))
        .await
        .map_err(|e| {
            tracing::error!("Get context recent join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get context recent failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_projects(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let storage = state.storage.clone();
    tokio::task::spawn_blocking(move || storage.get_all_projects())
        .await
        .map_err(|e| {
            tracing::error!("Get projects join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get projects failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<StorageStats>, StatusCode> {
    let storage = state.storage.clone();
    tokio::task::spawn_blocking(move || storage.get_stats())
        .await
        .map_err(|e| {
            tracing::error!("Get stats join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get stats failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_context_inject(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ContextQuery>,
) -> Result<Json<Vec<Observation>>, StatusCode> {
    let storage = state.storage.clone();
    let project = query.project.clone();
    let limit = query.limit;
    tokio::task::spawn_blocking(move || storage.get_context_for_project(&project, limit))
        .await
        .map_err(|e| {
            tracing::error!("Get context inject join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get context inject failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn sse_events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.event_tx.subscribe();
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(msg) => yield Ok(Event::default().data(msg)),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("SSE client lagged by {} messages", n);
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    Sse::new(stream)
}

pub async fn get_decisions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let q = if query.q.is_empty() {
        None
    } else {
        Some(query.q.clone())
    };
    let storage = state.storage.clone();
    let project = query.project.clone();
    let limit = query.limit;
    tokio::task::spawn_blocking(move || {
        storage.search_with_filters(q.as_deref(), project.as_deref(), Some("decision"), limit)
    })
    .await
    .map_err(|e| {
        tracing::error!("Get decisions join error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .map(Json)
    .map_err(|e| {
        tracing::error!("Get decisions failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn get_changes(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let q = if query.q.is_empty() {
        None
    } else {
        Some(query.q.clone())
    };
    let storage = state.storage.clone();
    let project = query.project.clone();
    let limit = query.limit;
    tokio::task::spawn_blocking(move || {
        storage.search_with_filters(q.as_deref(), project.as_deref(), Some("change"), limit)
    })
    .await
    .map_err(|e| {
        tracing::error!("Get changes join error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .map(Json)
    .map_err(|e| {
        tracing::error!("Get changes failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn get_how_it_works(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let search_query = if query.q.is_empty() {
        "how-it-works".to_string()
    } else {
        format!("{} how-it-works", query.q)
    };
    let storage = state.storage.clone();
    let limit = query.limit;
    tokio::task::spawn_blocking(move || storage.hybrid_search(&search_query, limit))
        .await
        .map_err(|e| {
            tracing::error!("Get how-it-works join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get how-it-works failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
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
    let storage = state.storage.clone();
    let project = query.project.clone();
    let limit = query.limit;
    let observations = tokio::task::spawn_blocking(move || {
        storage.get_context_for_project(&project, limit)
    })
    .await
    .map_err(|e| {
        tracing::error!("Context preview join error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .map_err(|e| {
        tracing::error!("Context preview failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
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
        observations
            .iter()
            .map(|o| format!("• {}", o.title))
            .collect::<Vec<_>>()
            .join("\n")
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
