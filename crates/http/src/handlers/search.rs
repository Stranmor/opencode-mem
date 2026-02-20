use crate::api_error::ApiError;
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use std::sync::Arc;

use opencode_mem_core::{
    MAX_QUERY_LIMIT, SearchResult, SessionSummary, UserPrompt, sort_by_score_descending,
};

use crate::AppState;
use crate::api_types::{
    FileSearchQuery, RankedItem, SearchQuery, TimelineResult, UnifiedSearchResult,
    UnifiedTimelineQuery,
};

pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    let q = if query.q.is_empty() { None } else { Some(query.q.as_str()) };

    // Use hybrid search if no exact filters are applied
    if query.project.is_none()
        && query.obs_type.is_none()
        && query.from.is_none()
        && query.to.is_none()
    {
        if let Some(query_str) = q {
            return state
                .search_service
                .hybrid_search(query_str, query.capped_limit())
                .await
                .map(Json)
                .map_err(|e| {
                    tracing::error!("Search error (hybrid fallback): {}", e);
                    ApiError::Internal(anyhow::anyhow!("Internal Error"))
                });
        }
    }

    state
        .search_service
        .search_with_filters(
            q,
            query.project.as_deref(),
            query.obs_type.as_deref(),
            query.from.as_deref(),
            query.to.as_deref(),
            query.capped_limit(),
        )
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search error: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        })
}

pub async fn hybrid_search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    state.search_service.hybrid_search(&query.q, query.capped_limit()).await.map(Json).map_err(
        |e| {
            tracing::error!("Hybrid search error: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        },
    )
}

pub async fn semantic_search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    if query.q.is_empty() {
        return Ok(Json(Vec::new()));
    }

    state
        .search_service
        .semantic_search_with_fallback(&query.q, query.capped_limit())
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Semantic search error: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        })
}

pub async fn search_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SessionSummary>>, ApiError> {
    if query.q.is_empty() {
        return Ok(Json(Vec::new()));
    }
    state.search_service.search_sessions(&query.q, query.capped_limit()).await.map(Json).map_err(
        |e| {
            tracing::error!("Search sessions error: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        },
    )
}

pub async fn search_prompts(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<UserPrompt>>, ApiError> {
    if query.q.is_empty() {
        return Ok(Json(Vec::new()));
    }
    state.search_service.search_prompts(&query.q, query.capped_limit()).await.map(Json).map_err(
        |e| {
            tracing::error!("Search prompts error: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        },
    )
}

pub async fn search_by_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FileSearchQuery>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    state
        .search_service
        .search_by_file(&query.file_path, query.limit.min(MAX_QUERY_LIMIT))
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search by file error: {}", e);
            ApiError::Internal(anyhow::anyhow!("Internal Error"))
        })
}

#[expect(
    clippy::cast_precision_loss,
    reason = "session/prompt counts never exceed f64 mantissa precision (2^53)"
)]
pub async fn unified_search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<UnifiedSearchResult>, ApiError> {
    if query.q.is_empty() {
        return Ok(Json(UnifiedSearchResult {
            observations: Vec::new(),
            sessions: Vec::new(),
            prompts: Vec::new(),
            ranked: Vec::new(),
        }));
    }
    let q = &query.q;
    let limit = query.capped_limit();

    let (obs_result, sess_result, prompt_result) = tokio::join!(
        state.search_service.search_with_filters(
            Some(q.as_str()),
            query.project.as_deref(),
            query.obs_type.as_deref(),
            query.from.as_deref(),
            query.to.as_deref(),
            limit,
        ),
        state.search_service.search_sessions(q, limit),
        state.search_service.search_prompts(q, limit),
    );

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
    sort_by_score_descending(&mut ranked);

    Ok(Json(UnifiedSearchResult { observations, sessions, prompts, ranked }))
}

pub async fn unified_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UnifiedTimelineQuery>,
) -> Result<Json<TimelineResult>, ApiError> {
    let anchor_obs = if let Some(ref id) = query.anchor {
        state.search_service.get_observation_by_id(id).await.ok().flatten()
    } else if let Some(ref q) = query.q {
        let search_result =
            state.search_service.hybrid_search(q, 1).await.ok().and_then(|r| r.into_iter().next());
        if let Some(sr) = search_result {
            state.search_service.get_observation_by_id(&sr.id).await.ok().flatten()
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

        let (before_result, after_result) = tokio::join!(
            state.search_service.get_timeline(None, Some(&anchor_time), before_limit),
            state.search_service.get_timeline(Some(&anchor_time), None, after_limit),
        );

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
