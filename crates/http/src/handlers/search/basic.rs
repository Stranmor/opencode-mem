use crate::api_error::ApiError;
use axum::{
    Json,
    extract::{Query, State},
};
use std::sync::Arc;

use opencode_mem_core::{SearchResult, SessionSummary, UserPrompt};

use crate::AppState;
use crate::api_types::{FileSearchQuery, SearchQuery};

pub async fn search(
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
        .smart_search(
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
            ApiError::from(e)
        })
}

pub async fn hybrid_search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    state
        .search_service
        .hybrid_search(&query.q, query.capped_limit())
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Hybrid search error: {}", e);
            ApiError::from(e)
        })
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
            ApiError::from(e)
        })
}

pub async fn search_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SessionSummary>>, ApiError> {
    if query.q.is_empty() {
        return Ok(Json(Vec::new()));
    }
    state
        .search_service
        .search_sessions(&query.q, query.capped_limit())
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search sessions error: {}", e);
            ApiError::from(e)
        })
}

pub async fn search_prompts(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<UserPrompt>>, ApiError> {
    if query.q.is_empty() {
        return Ok(Json(Vec::new()));
    }
    state
        .search_service
        .search_prompts(&query.q, query.capped_limit())
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search prompts error: {}", e);
            ApiError::from(e)
        })
}

pub async fn search_by_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FileSearchQuery>,
) -> Result<Json<Vec<SearchResult>>, ApiError> {
    state
        .search_service
        .search_by_file(&query.file_path, query.limit)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search by file error: {}", e);
            ApiError::from(e)
        })
}
