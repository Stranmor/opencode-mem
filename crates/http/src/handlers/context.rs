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
use opencode_mem_storage::traits::InjectionStore;
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
    let observations =
        state.search_service.get_context_for_project(&query.project, query.limit).await.map_err(
            |e| {
                tracing::error!("Get context error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            },
        )?;

    if let Some(ref session_id) = query.session_id {
        let ids: Vec<String> = observations.iter().map(|o| o.id.clone()).collect();
        if !ids.is_empty() {
            if let Err(e) = state.storage.save_injected_observations(session_id, &ids).await {
                tracing::warn!("Failed to record injected observations: {}", e);
            }
        }
    }

    Ok(Json(observations))
}

pub async fn get_projects(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<String>>, StatusCode> {
    state.search_service.get_all_projects().await.map(Json).map_err(|e| {
        tracing::error!("Get projects error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn get_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<StorageStats>, StatusCode> {
    state.search_service.get_stats().await.map(Json).map_err(|e| {
        tracing::error!("Get stats error: {}", e);
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
    let q = if query.q.is_empty() { None } else { Some(query.q.as_str()) };
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
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get decisions error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_changes(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let q = if query.q.is_empty() { None } else { Some(query.q.as_str()) };
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
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get changes error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
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
    state.search_service.hybrid_search(&search_query, query.capped_limit()).await.map(Json).map_err(
        |e| {
            tracing::error!("How it works search error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        },
    )
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
    let observations =
        state.search_service.get_context_for_project(&query.project, query.limit).await.map_err(
            |e| {
                tracing::error!("Context preview error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            },
        )?;
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

pub async fn search_help() -> Json<SearchHelpResponse> {
    Json(get_search_help())
}
