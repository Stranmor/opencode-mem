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
use crate::blocking::{blocking_json, blocking_result};
use crate::AppState;

pub async fn search(
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
    let obs_type = query.obs_type.clone();
    let limit = query.limit;
    blocking_json(move || {
        storage.search_with_filters(q.as_deref(), project.as_deref(), obs_type.as_deref(), limit)
    })
    .await
}

pub async fn hybrid_search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let storage = state.storage.clone();
    let q = query.q.clone();
    let limit = query.limit;
    blocking_json(move || storage.hybrid_search(&q, limit)).await
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
            Ok(query_vec) => {
                let storage = state.storage.clone();
                let limit = query.limit;
                match blocking_result(move || storage.semantic_search(&query_vec, limit)).await {
                    Ok(results) if !results.is_empty() => Ok(Json(results)),
                    Ok(_) => {
                        let storage = state.storage.clone();
                        let q = query.q.clone();
                        let limit = query.limit;
                        blocking_json(move || storage.hybrid_search(&q, limit)).await
                    }
                    Err(e) => Err(e),
                }
            }
            Err(e) => {
                tracing::warn!("Failed to embed query, falling back to hybrid: {}", e);
                let storage = state.storage.clone();
                let q = query.q.clone();
                let limit = query.limit;
                blocking_json(move || storage.hybrid_search(&q, limit)).await
            }
        },
        None => {
            let storage = state.storage.clone();
            let q = query.q.clone();
            let limit = query.limit;
            blocking_json(move || storage.hybrid_search(&q, limit)).await
        }
    }
}

pub async fn search_observations(
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
    blocking_json(move || storage.search_with_filters(q.as_deref(), project.as_deref(), None, limit))
        .await
}

pub async fn search_by_type(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let storage = state.storage.clone();
    let project = query.project.clone();
    let obs_type = query.obs_type.clone();
    let limit = query.limit;
    blocking_json(move || {
        storage.search_with_filters(None, project.as_deref(), obs_type.as_deref(), limit)
    })
    .await
}

pub async fn search_by_concept(
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
    blocking_json(move || storage.search_with_filters(q.as_deref(), project.as_deref(), None, limit))
        .await
}

pub async fn search_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SessionSummary>>, StatusCode> {
    if query.q.is_empty() {
        return Ok(Json(Vec::new()));
    }
    let storage = state.storage.clone();
    let q = query.q.clone();
    let limit = query.limit;
    blocking_json(move || storage.search_sessions(&q, limit)).await
}

pub async fn search_prompts(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<UserPrompt>>, StatusCode> {
    if query.q.is_empty() {
        return Ok(Json(Vec::new()));
    }
    let storage = state.storage.clone();
    let q = query.q.clone();
    let limit = query.limit;
    blocking_json(move || storage.search_prompts(&q, limit)).await
}

pub async fn search_by_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FileSearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let storage = state.storage.clone();
    let file_path = query.file_path.clone();
    let limit = query.limit;
    blocking_json(move || storage.search_by_file(&file_path, limit)).await
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
    let storage = state.storage.clone();
    let q = query.q.clone();
    let project = query.project.clone();
    let obs_type = query.obs_type.clone();
    let limit = query.limit;
    let observations = tokio::task::spawn_blocking({
        let storage = storage.clone();
        let q = q.clone();
        let project = project.clone();
        let obs_type = obs_type.clone();
        move || storage.search_with_filters(Some(&q), project.as_deref(), obs_type.as_deref(), limit)
    })
    .await
    .map_err(|e| {
        tracing::error!("Unified search observations join error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .unwrap_or_default();
    let sessions = tokio::task::spawn_blocking({
        let storage = storage.clone();
        let q = q.clone();
        move || storage.search_sessions(&q, limit)
    })
    .await
    .map_err(|e| {
        tracing::error!("Unified search sessions join error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .unwrap_or_default();
    let prompts = tokio::task::spawn_blocking({
        let storage = storage.clone();
        let q = q.clone();
        move || storage.search_prompts(&q, limit)
    })
    .await
    .map_err(|e| {
        tracing::error!("Unified search prompts join error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
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
        let storage = state.storage.clone();
        let id = id.clone();
        tokio::task::spawn_blocking(move || storage.get_by_id(&id))
            .await
            .ok()
            .and_then(|r| r.ok())
            .flatten()
    } else if let Some(q) = &query.q {
        let storage = state.storage.clone();
        let q = q.clone();
        let search_result = tokio::task::spawn_blocking(move || storage.hybrid_search(&q, 1))
            .await
            .ok()
            .and_then(|r| r.ok())
            .and_then(|r| r.into_iter().next());
        if let Some(sr) = search_result {
            let storage = state.storage.clone();
            let id = sr.id;
            tokio::task::spawn_blocking(move || storage.get_by_id(&id))
                .await
                .ok()
                .and_then(|r| r.ok())
                .flatten()
        } else {
            None
        }
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

        let storage = state.storage.clone();
        let anchor_time_clone = anchor_time.clone();
        let before_limit = query.before + 1;
        let anchor_id = anchor_sr.id.clone();
        let before_items = tokio::task::spawn_blocking(move || {
            storage.get_timeline(None, Some(&anchor_time_clone), before_limit)
        })
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|o| o.id != anchor_id)
        .take(query.before)
        .collect();

        let storage = state.storage.clone();
        let after_limit = query.after + 1;
        let anchor_id = anchor_sr.id.clone();
        let after_items: Vec<_> = tokio::task::spawn_blocking(move || {
            storage.get_timeline(Some(&anchor_time), None, after_limit)
        })
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|o| o.id != anchor_id)
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
