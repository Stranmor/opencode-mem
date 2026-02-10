use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use tokio::task::spawn_blocking;

use opencode_mem_core::{SearchResult, SessionSummary, UserPrompt};

use crate::api_types::{
    FileSearchQuery, RankedItem, SearchQuery, TimelineResult, UnifiedSearchResult,
    UnifiedTimelineQuery,
};
use crate::blocking::blocking_json;
use crate::AppState;

pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let q = if query.q.is_empty() { None } else { Some(query.q.clone()) };
    let storage = Arc::clone(&state.storage);
    let project = query.project.clone();
    let obs_type = query.obs_type.clone();
    let from = query.from.clone();
    let to = query.to.clone();
    let limit = query.limit;
    blocking_json(move || {
        storage.search_with_filters(
            q.as_deref(),
            project.as_deref(),
            obs_type.as_deref(),
            from.as_deref(),
            to.as_deref(),
            limit,
        )
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

    let storage = Arc::clone(&state.storage);
    let embeddings = state.embeddings.clone();
    let q = query.q.clone();
    let limit = query.limit;
    blocking_json(move || {
        opencode_mem_search::run_semantic_search_with_fallback(
            &storage,
            embeddings.as_deref(),
            &q,
            limit,
        )
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
            ranked: Vec::new(),
        }));
    }
    let storage = Arc::clone(&state.storage);
    let q = query.q.clone();
    let project = query.project.clone();
    let obs_type = query.obs_type.clone();
    let from = query.from.clone();
    let to = query.to.clone();
    let limit = query.limit;
    let obs_handle = spawn_blocking({
        let storage = Arc::clone(&storage);
        let q = q.clone();
        let project = project.clone();
        let obs_type = obs_type.clone();
        let from = from.clone();
        let to = to.clone();
        move || {
            storage.search_with_filters(
                Some(&q),
                project.as_deref(),
                obs_type.as_deref(),
                from.as_deref(),
                to.as_deref(),
                limit,
            )
        }
    });
    let sess_handle = spawn_blocking({
        let storage = Arc::clone(&storage);
        let q = q.clone();
        move || storage.search_sessions(&q, limit)
    });
    let prompt_handle = spawn_blocking({
        let storage = Arc::clone(&storage);
        let q = q.clone();
        move || storage.search_prompts(&q, limit)
    });

    let (obs_result, sess_result, prompt_result) =
        tokio::try_join!(obs_handle, sess_handle, prompt_handle).map_err(|e| {
            tracing::error!("Unified search join error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let observations = obs_result.unwrap_or_else(|e| {
        tracing::error!("Unified search observation query failed: {e}");
        Vec::new()
    });
    let sessions = sess_result.unwrap_or_else(|e| {
        tracing::error!("Unified search session query failed: {e}");
        Vec::new()
    });
    let prompts = prompt_result.unwrap_or_else(|e| {
        tracing::error!("Unified search prompt query failed: {e}");
        Vec::new()
    });
    // Build ranked list from all collections
    let mut ranked: Vec<RankedItem> = Vec::new();

    // Observations already have relevance scores
    for obs in &observations {
        ranked.push(RankedItem {
            id: obs.id.clone(),
            title: obs.title.clone(),
            subtitle: obs.subtitle.clone(),
            collection: "observation".to_owned(),
            score: obs.score,
        });
    }

    // Sessions: position-based scoring (first = 1.0, last = 0.1)
    for (i, session) in sessions.iter().enumerate() {
        let position_score = if sessions.len() <= 1 {
            1.0
        } else {
            1.0 - (i as f64 / (sessions.len() - 1) as f64) * 0.9
        };
        ranked.push(RankedItem {
            id: session.session_id.clone(),
            title: session.request.clone().unwrap_or_default(),
            subtitle: Some(session.project.clone()),
            collection: "session".to_owned(),
            score: position_score,
        });
    }

    // Prompts: position-based scoring
    for (i, prompt) in prompts.iter().enumerate() {
        let position_score = if prompts.len() <= 1 {
            1.0
        } else {
            1.0 - (i as f64 / (prompts.len() - 1) as f64) * 0.9
        };
        ranked.push(RankedItem {
            id: prompt.id.clone(),
            title: prompt.prompt_text.chars().take(100).collect(),
            subtitle: prompt.project.clone(),
            collection: "prompt".to_owned(),
            score: position_score,
        });
    }

    // Sort by score descending
    ranked.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    Ok(Json(UnifiedSearchResult { observations, sessions, prompts, ranked }))
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

        let before_limit = query.before.saturating_add(1);
        let after_limit = query.after.saturating_add(1);

        let before_handle = spawn_blocking({
            let storage = Arc::clone(&state.storage);
            let anchor_time_clone = anchor_time.clone();
            move || storage.get_timeline(None, Some(&anchor_time_clone), before_limit)
        });
        let after_handle = spawn_blocking({
            let storage = Arc::clone(&state.storage);
            let anchor_time_clone = anchor_time.clone();
            move || storage.get_timeline(Some(&anchor_time_clone), None, after_limit)
        });

        let (before_result, after_result) =
            tokio::try_join!(before_handle, after_handle).map_err(|e| {
                tracing::error!("Unified timeline join error: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let anchor_id = anchor_sr.id.clone();
        let before_items = before_result
            .unwrap_or_else(|e| {
                tracing::error!("Unified timeline before query failed: {e}");
                Vec::new()
            })
            .into_iter()
            .filter(|o| o.id != anchor_id)
            .take(query.before)
            .collect();

        let after_items: Vec<_> = after_result
            .unwrap_or_else(|e| {
                tracing::error!("Unified timeline after query failed: {e}");
                Vec::new()
            })
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
