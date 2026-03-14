use crate::api_error::{ApiError, OrDegraded};
use axum::{
    Json,
    extract::{Query, State},
};
use std::sync::Arc;

use opencode_mem_core::{SearchResult, sort_by_score_descending};

use crate::AppState;
use crate::api_types::{
    RankedItem, SearchQuery, TimelineResult, UnifiedSearchResult, UnifiedTimelineQuery,
};

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

    // If any query failed due to database unavailability, return a degraded response.
    // This ensures consistency with other endpoints and provides the X-Memory-Degraded header.
    let degraded_body = || -> serde_json::Value {
        serde_json::to_value(UnifiedSearchResult::default()).unwrap_or(serde_json::Value::Null)
    };

    if let Err(e) = &obs_result
        && (e.is_db_unavailable() || e.is_transient())
    {
        return Err(ApiError::Degraded(degraded_body()));
    }
    if let Err(e) = &sess_result
        && (e.is_db_unavailable() || e.is_transient())
    {
        return Err(ApiError::Degraded(degraded_body()));
    }
    if let Err(e) = &prompt_result
        && (e.is_db_unavailable() || e.is_transient())
    {
        return Err(ApiError::Degraded(degraded_body()));
    }

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

    let mut ranked: Vec<RankedItem> = Vec::new();

    for obs in &observations {
        ranked.push(RankedItem {
            id: obs.id.to_string(),
            title: obs.title.clone(),
            subtitle: obs.subtitle.clone(),
            collection: "observation".to_owned(),
            score: obs.score,
        });
    }

    for (i, session) in sessions.iter().enumerate() {
        let position_score = if sessions.len() <= 1 {
            1.0
        } else {
            1.0 - (i as f64 / (sessions.len() - 1) as f64) * 0.9
        };
        ranked.push(RankedItem {
            id: session.session_id.to_string(),
            title: session.request.clone().unwrap_or_default(),
            subtitle: Some(session.project.to_string()),
            collection: "session".to_owned(),
            score: position_score,
        });
    }

    for (i, prompt) in prompts.iter().enumerate() {
        let position_score = if prompts.len() <= 1 {
            1.0
        } else {
            1.0 - (i as f64 / (prompts.len() - 1) as f64) * 0.9
        };
        ranked.push(RankedItem {
            id: prompt.id.clone(),
            title: prompt.prompt_text.chars().take(100).collect(),
            subtitle: prompt.project.as_ref().map(|p| p.to_string()),
            collection: "prompt".to_owned(),
            score: position_score,
        });
    }

    sort_by_score_descending(&mut ranked);

    Ok(Json(UnifiedSearchResult {
        observations,
        sessions,
        prompts,
        ranked,
    }))
}

pub async fn unified_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UnifiedTimelineQuery>,
) -> Result<Json<TimelineResult>, ApiError> {
    let anchor_obs = if let Some(ref id) = query.anchor {
        state
            .search_service
            .get_observation_by_id(id)
            .await
            .map_err(|e| {
                tracing::warn!(
                    error = %e,
                    "Failed to fetch anchor observation"
                );
            })
            .ok()
            .flatten()
    } else if let Some(ref q) = query.q {
        let search_result = state
            .search_service
            .hybrid_search(q, 1)
            .await
            .map_err(|e| {
                tracing::warn!(
                    error = %e,
                    "Failed to perform hybrid search for anchor"
                );
            })
            .ok()
            .and_then(|r| r.into_iter().next());
        if let Some(sr) = search_result {
            state
                .search_service
                .get_observation_by_id(&sr.id)
                .await
                .map_err(|e| {
                    tracing::warn!(
                        error = %e,
                        "Failed to fetch observation from \
                         hybrid search result"
                    );
                })
                .ok()
                .flatten()
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
            state
                .search_service
                .get_timeline(None, Some(&anchor_time), before_limit),
            state
                .search_service
                .get_timeline(Some(&anchor_time), None, after_limit),
        );

        // If any query failed due to database unavailability, return a degraded response.
        let degraded_timeline = || -> serde_json::Value {
            serde_json::to_value(TimelineResult::default()).unwrap_or(serde_json::Value::Null)
        };

        if let Err(e) = &before_result
            && (e.is_db_unavailable() || e.is_transient())
        {
            return Err(ApiError::Degraded(degraded_timeline()));
        }
        if let Err(e) = &after_result
            && (e.is_db_unavailable() || e.is_transient())
        {
            return Err(ApiError::Degraded(degraded_timeline()));
        }

        let anchor_id = anchor_sr.id.clone();
        let before_items = before_result
            .unwrap_or_else(|e| {
                tracing::error!(
                    "Unified timeline before query \
                         failed: {e}"
                );
                Vec::new()
            })
            .into_iter()
            .filter(|o| o.id != anchor_id)
            .take(query.before)
            .collect();

        let after_items: Vec<_> = after_result
            .unwrap_or_else(|e| {
                tracing::error!(
                    "Unified timeline after query \
                         failed: {e}"
                );
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

    Ok(Json(TimelineResult {
        anchor: anchor_sr,
        before,
        after,
    }))
}
