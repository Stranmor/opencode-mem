use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use opencode_mem_core::{SearchResult, SessionSummary, UserPrompt};
use opencode_mem_embeddings::EmbeddingProvider;

use crate::api_types::{
    FileSearchQuery, SearchQuery, TimelineResult, UnifiedSearchResult, UnifiedTimelineQuery,
};
use crate::AppState;

pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let q = if query.q.is_empty() {
        None
    } else {
        Some(query.q.as_str())
    };
    state
        .storage
        .search_with_filters(
            q,
            query.project.as_deref(),
            query.obs_type.as_deref(),
            query.limit,
        )
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn hybrid_search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    state
        .storage
        .hybrid_search(&query.q, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Hybrid search failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn semantic_search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    if query.q.is_empty() {
        return Ok(Json(Vec::new()));
    }

    match &state.embeddings {
        Some(emb) => match emb.embed(&query.q) {
            Ok(query_vec) => match state.storage.semantic_search(&query_vec, query.limit) {
                Ok(results) if !results.is_empty() => Ok(Json(results)),
                Ok(_) => state
                    .storage
                    .hybrid_search(&query.q, query.limit)
                    .map(Json)
                    .map_err(|e| {
                        tracing::error!("Fallback hybrid search failed: {}", e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    }),
                Err(e) => {
                    tracing::error!("Semantic search failed: {}", e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            },
            Err(e) => {
                tracing::warn!("Failed to embed query, falling back to hybrid: {}", e);
                state
                    .storage
                    .hybrid_search(&query.q, query.limit)
                    .map(Json)
                    .map_err(|e| {
                        tracing::error!("Fallback hybrid search failed: {}", e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })
            }
        },
        None => state
            .storage
            .hybrid_search(&query.q, query.limit)
            .map(Json)
            .map_err(|e| {
                tracing::error!("Hybrid search (no embeddings) failed: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            }),
    }
}

pub async fn search_observations(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let q = if query.q.is_empty() {
        None
    } else {
        Some(query.q.as_str())
    };
    state
        .storage
        .search_with_filters(q, query.project.as_deref(), None, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search observations failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn search_by_type(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    state
        .storage
        .search_with_filters(
            None,
            query.project.as_deref(),
            query.obs_type.as_deref(),
            query.limit,
        )
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search by type failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn search_by_concept(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let q = if query.q.is_empty() {
        None
    } else {
        Some(query.q.as_str())
    };
    state
        .storage
        .search_with_filters(q, query.project.as_deref(), None, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search by concept failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn search_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SessionSummary>>, StatusCode> {
    if query.q.is_empty() {
        return Ok(Json(Vec::new()));
    }
    state
        .storage
        .search_sessions(&query.q, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search sessions failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn search_prompts(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<UserPrompt>>, StatusCode> {
    if query.q.is_empty() {
        return Ok(Json(Vec::new()));
    }
    state
        .storage
        .search_prompts(&query.q, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search prompts failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn search_by_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FileSearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    state
        .storage
        .search_by_file(&query.file_path, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search by file failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn unified_search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<UnifiedSearchResult>, StatusCode> {
    if query.q.is_empty() {
        return Ok(Json(UnifiedSearchResult {
            observations: Vec::new(),
            sessions: Vec::new(),
            prompts: Vec::new(),
        }));
    }
    let observations = state
        .storage
        .search_with_filters(
            Some(&query.q),
            query.project.as_deref(),
            query.obs_type.as_deref(),
            query.limit,
        )
        .unwrap_or_default();
    let sessions = state
        .storage
        .search_sessions(&query.q, query.limit)
        .unwrap_or_default();
    let prompts = state
        .storage
        .search_prompts(&query.q, query.limit)
        .unwrap_or_default();
    Ok(Json(UnifiedSearchResult {
        observations,
        sessions,
        prompts,
    }))
}

pub async fn unified_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UnifiedTimelineQuery>,
) -> Result<Json<TimelineResult>, StatusCode> {
    let anchor_obs = if let Some(id) = &query.anchor {
        state.storage.get_by_id(id).ok().flatten()
    } else if let Some(q) = &query.q {
        state
            .storage
            .hybrid_search(q, 1)
            .ok()
            .and_then(|r| r.into_iter().next())
            .and_then(|sr| state.storage.get_by_id(&sr.id).ok().flatten())
    } else {
        None
    };

    let (anchor_sr, before, after) = if let Some(obs) = anchor_obs {
        let anchor_time = obs.created_at.to_rfc3339();
        let anchor_sr = SearchResult {
            id: obs.id,
            title: obs.title,
            subtitle: obs.subtitle.clone(),
            observation_type: obs.observation_type,
            score: 1.0,
        };

        let before_items = state
            .storage
            .get_timeline(None, Some(&anchor_time), query.before + 1)
            .unwrap_or_default()
            .into_iter()
            .filter(|o| o.id != anchor_sr.id)
            .take(query.before)
            .collect();

        let after_items: Vec<_> = state
            .storage
            .get_timeline(Some(&anchor_time), None, query.after + 1)
            .unwrap_or_default()
            .into_iter()
            .filter(|o| o.id != anchor_sr.id)
            .take(query.after)
            .collect();

        (Some(anchor_sr), before_items, after_items)
    } else {
        (None, Vec::new(), Vec::new())
    };

    Ok(Json(TimelineResult {
        anchor: anchor_sr,
        before,
        after,
    }))
}
