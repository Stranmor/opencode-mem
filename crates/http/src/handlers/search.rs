use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use tokio::task::spawn_blocking;

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
    let q = if query.q.is_empty() { None } else { Some(query.q.clone()) };
    let storage = Arc::clone(&state.storage);
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
    let storage = Arc::clone(&state.storage);
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

    if let Some(emb) = state.embeddings.as_ref() {
        match emb.embed(&query.q) {
            Ok(query_vec) => {
                let storage = Arc::clone(&state.storage);
                let limit = query.limit;
                match blocking_result(move || storage.semantic_search(&query_vec, limit)).await {
                    Ok(results) if !results.is_empty() => Ok(Json(results)),
                    Ok(_) => {
                        let storage = Arc::clone(&state.storage);
                        let q = query.q.clone();
                        let limit = query.limit;
                        blocking_json(move || storage.hybrid_search(&q, limit)).await
                    },
                    Err(e) => {
                        tracing::warn!(
                            "Semantic search storage error, falling back to hybrid: {}",
                            e
                        );
                        let storage = Arc::clone(&state.storage);
                        let q = query.q.clone();
                        let limit = query.limit;
                        blocking_json(move || storage.hybrid_search(&q, limit)).await
                    },
                }
            },
            Err(e) => {
                tracing::warn!("Failed to embed query, falling back to hybrid: {}", e);
                let storage = Arc::clone(&state.storage);
                let q = query.q.clone();
                let limit = query.limit;
                blocking_json(move || storage.hybrid_search(&q, limit)).await
            },
        }
    } else {
        let storage = Arc::clone(&state.storage);
        let q = query.q.clone();
        let limit = query.limit;
        blocking_json(move || storage.hybrid_search(&q, limit)).await
    }
}

pub async fn search_observations(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let q = if query.q.is_empty() { None } else { Some(query.q.clone()) };
    let storage = Arc::clone(&state.storage);
    let project = query.project.clone();
    let limit = query.limit;
    blocking_json(move || {
        storage.search_with_filters(q.as_deref(), project.as_deref(), None, limit)
    })
    .await
}

pub async fn search_by_type(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let storage = Arc::clone(&state.storage);
    let project = query.project.clone();
    let obs_type = query.obs_type.clone();
    let limit = query.limit;
    blocking_json(move || {
        storage.search_with_filters(None, project.as_deref(), obs_type.as_deref(), limit)
    })
    .await
}

pub async fn search_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SessionSummary>>, StatusCode> {
    if query.q.is_empty() {
        return Ok(Json(Vec::new()));
    }
    let storage = Arc::clone(&state.storage);
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
    let storage = Arc::clone(&state.storage);
    let q = query.q.clone();
    let limit = query.limit;
    blocking_json(move || storage.search_prompts(&q, limit)).await
}

pub async fn search_by_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FileSearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let storage = Arc::clone(&state.storage);
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
    let storage = Arc::clone(&state.storage);
    let q = query.q.clone();
    let project = query.project.clone();
    let obs_type = query.obs_type.clone();
    let limit = query.limit;
    let observations = spawn_blocking({
        let storage = Arc::clone(&storage);
        let q = q.clone();
        let project = project.clone();
        let obs_type = obs_type.clone();
        move || {
            storage.search_with_filters(Some(&q), project.as_deref(), obs_type.as_deref(), limit)
        }
    })
    .await
    .map_err(|e| {
        tracing::error!("Unified search observations join error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .unwrap_or_default();
    let sessions = spawn_blocking({
        let storage = Arc::clone(&storage);
        let q = q.clone();
        move || storage.search_sessions(&q, limit)
    })
    .await
    .map_err(|e| {
        tracing::error!("Unified search sessions join error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .unwrap_or_default();
    let prompts = spawn_blocking({
        let storage = Arc::clone(&storage);
        let q = q.clone();
        move || storage.search_prompts(&q, limit)
    })
    .await
    .map_err(|e| {
        tracing::error!("Unified search prompts join error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .unwrap_or_default();
    Ok(Json(UnifiedSearchResult { observations, sessions, prompts }))
}

pub async fn unified_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UnifiedTimelineQuery>,
) -> Result<Json<TimelineResult>, StatusCode> {
    let anchor_obs = if let Some(ref id) = query.anchor {
        let storage = Arc::clone(&state.storage);
        let id = id.clone();
        spawn_blocking(move || storage.get_by_id(&id)).await.ok().and_then(Result::ok).flatten()
    } else if let Some(ref q) = query.q {
        let storage = Arc::clone(&state.storage);
        let q = q.clone();
        let search_result = spawn_blocking(move || storage.hybrid_search(&q, 1))
            .await
            .ok()
            .and_then(Result::ok)
            .and_then(|r| r.into_iter().next());
        if let Some(sr) = search_result {
            let storage = Arc::clone(&state.storage);
            let id = sr.id;
            spawn_blocking(move || storage.get_by_id(&id)).await.ok().and_then(Result::ok).flatten()
        } else {
            None
        }
    } else {
        None
    };

    let (anchor_sr, before, after) = if let Some(obs) = anchor_obs {
        let anchor_time = obs.created_at.to_rfc3339();
        let anchor_sr = SearchResult::new(
            obs.id,
            obs.title,
            obs.subtitle.clone(),
            obs.observation_type,
            obs.noise_level,
            1.0,
        );

        let storage = Arc::clone(&state.storage);
        let anchor_time_clone = anchor_time.clone();
        let before_limit = query.before.saturating_add(1);
        let anchor_id = anchor_sr.id.clone();
        let before_items = spawn_blocking(move || {
            storage.get_timeline(None, Some(&anchor_time_clone), before_limit)
        })
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default()
        .into_iter()
        .filter(|o| o.id != anchor_id)
        .take(query.before)
        .collect();

        let storage = Arc::clone(&state.storage);
        let after_limit = query.after.saturating_add(1);
        let anchor_id = anchor_sr.id.clone();
        let after_items: Vec<_> =
            spawn_blocking(move || storage.get_timeline(Some(&anchor_time), None, after_limit))
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default()
                .into_iter()
                .filter(|o| o.id != anchor_id)
                .take(query.after)
                .collect();

        (Some(anchor_sr), before_items, after_items)
    } else {
        (None, Vec::new(), Vec::new())
    };

    Ok(Json(TimelineResult { anchor: anchor_sr, before, after }))
}
