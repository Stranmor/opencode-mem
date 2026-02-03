//! HTTP API server (Axum)
use axum::{
    Router,
    routing::{get, post},
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
    response::sse::{Event, Sse},
};
use std::sync::Arc;
use tokio::sync::{broadcast, Semaphore};
use tower_http::cors::CorsLayer;
use futures_util::stream::Stream;
use std::convert::Infallible;
use serde::{Deserialize, Serialize};

use opencode_mem_core::{
    Observation, SearchResult, ObservationInput, ToolOutput, ToolCall, SessionSummary, UserPrompt,
};
use opencode_mem_storage::{PaginatedResult, Storage, StorageStats};
use opencode_mem_llm::LlmClient;

#[derive(Debug, Serialize, Deserialize)]
pub struct ObserveResponse {
    pub id: String,
    pub queued: bool,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub project: Option<String>,
    #[serde(rename = "type")]
    pub obs_type: Option<String>,
}

fn default_limit() -> usize {
    20
}

#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct ContextQuery {
    pub project: String,
    #[serde(default = "default_context_limit")]
    pub limit: usize,
}

fn default_context_limit() -> usize {
    50
}

#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SessionSummaryRequest {
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub project: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct VersionResponse {
    pub version: &'static str,
}

pub struct AppState {
    pub storage: Storage,
    pub llm: LlmClient,
    pub semaphore: Arc<Semaphore>,
    pub event_tx: broadcast::Sender<String>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/readiness", get(readiness))
        .route("/api/version", get(version))
        .route("/observe", post(observe))
        .route("/search", get(search))
        .route("/hybrid-search", get(hybrid_search))
        .route("/observations/{id}", get(get_observation))
        .route("/observations/batch", post(get_observations_batch))
        .route("/api/observations", get(get_observations_paginated))
        .route("/api/summaries", get(get_summaries_paginated))
        .route("/api/prompts", get(get_prompts_paginated))
        .route("/api/session/{id}", get(get_session_by_id))
        .route("/api/prompt/{id}", get(get_prompt_by_id))
        .route("/api/search/observations", get(search_observations))
        .route("/api/search/by-type", get(search_by_type))
        .route("/api/search/by-concept", get(search_by_concept))
        .route("/api/search/sessions", get(search_sessions))
        .route("/api/search/prompts", get(search_prompts))
        .route("/api/search/by-file", get(search_by_file))
        .route("/api/context/recent", get(get_context_recent))
        .route("/recent", get(get_recent))
        .route("/timeline", get(get_timeline))
        .route("/projects", get(get_projects))
        .route("/stats", get(get_stats))
        .route("/context/inject", get(get_context_inject))
        .route("/session/summary", post(generate_summary))
        .route("/events", get(sse_events))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn readiness() -> (StatusCode, Json<ReadinessResponse>) {
    (StatusCode::OK, Json(ReadinessResponse {
        status: "ready",
        message: None,
    }))
}

async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn observe(
    State(state): State<Arc<AppState>>,
    Json(tool_call): Json<ToolCall>,
) -> Result<Json<ObserveResponse>, StatusCode> {
    let id = uuid::Uuid::new_v4().to_string();
    
    let state_clone = state.clone();
    let id_clone = id.clone();
    tokio::spawn(async move {
        let permit = match state_clone.semaphore.acquire().await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Semaphore closed, cannot process observation: {}", e);
                return;
            }
        };
        if let Err(e) = process_observation(&state_clone, &id_clone, tool_call).await {
            tracing::error!("Failed to process observation: {}", e);
        }
        drop(permit);
    });

    Ok(Json(ObserveResponse { id, queued: true }))
}

async fn process_observation(state: &AppState, id: &str, tool_call: ToolCall) -> anyhow::Result<()> {
    let input = ObservationInput {
        tool: tool_call.tool.clone(),
        session_id: tool_call.session_id.clone(),
        call_id: tool_call.call_id.clone(),
        output: ToolOutput {
            title: format!("Observation from {}", tool_call.tool),
            output: tool_call.output,
            metadata: tool_call.input,
        },
    };
    let observation = state.llm.compress_to_observation(id, &input, tool_call.project.as_deref()).await?;
    state.storage.save_observation(&observation)?;
    tracing::info!("Saved observation: {} - {}", observation.id, observation.title);
    let _ = state.event_tx.send(serde_json::to_string(&observation)?);
    Ok(())
}

async fn search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let q = if query.q.is_empty() { None } else { Some(query.q.as_str()) };
    state.storage.search_with_filters(q, query.project.as_deref(), query.obs_type.as_deref(), query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn hybrid_search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    state.storage.hybrid_search(&query.q, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Hybrid search failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_observation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<Observation>>, StatusCode> {
    state.storage.get_by_id(&id)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get observation failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_recent(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    state.storage.get_recent(query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get recent failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    state.storage.get_timeline(query.from.as_deref(), query.to.as_deref(), query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get timeline failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_observations_batch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchRequest>,
) -> Result<Json<Vec<Observation>>, StatusCode> {
    state.storage.get_observations_by_ids(&req.ids)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Batch get observations failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_observations_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<Observation>>, StatusCode> {
    let limit = query.limit.min(100);
    state.storage.get_observations_paginated(query.offset, limit, query.project.as_deref())
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get observations paginated failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_summaries_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<SessionSummary>>, StatusCode> {
    let limit = query.limit.min(100);
    state.storage.get_summaries_paginated(query.offset, limit, query.project.as_deref())
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get summaries paginated failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_prompts_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<UserPrompt>>, StatusCode> {
    let limit = query.limit.min(100);
    state.storage.get_prompts_paginated(query.offset, limit, query.project.as_deref())
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get prompts paginated failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_session_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<SessionSummary>>, StatusCode> {
    state.storage.get_session_summary(&id)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get session by id failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_prompt_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<UserPrompt>>, StatusCode> {
    state.storage.get_prompt_by_id(&id)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get prompt by id failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn search_observations(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let q = if query.q.is_empty() { None } else { Some(query.q.as_str()) };
    state.storage.search_with_filters(q, query.project.as_deref(), None, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search observations failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn search_by_type(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    state.storage.search_with_filters(None, query.project.as_deref(), query.obs_type.as_deref(), query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search by type failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn search_by_concept(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let q = if query.q.is_empty() { None } else { Some(query.q.as_str()) };
    state.storage.search_with_filters(q, query.project.as_deref(), None, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search by concept failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn search_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SessionSummary>>, StatusCode> {
    if query.q.is_empty() {
        return Ok(Json(Vec::new()));
    }
    state.storage.search_sessions(&query.q, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search sessions failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn search_prompts(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<UserPrompt>>, StatusCode> {
    if query.q.is_empty() {
        return Ok(Json(Vec::new()));
    }
    state.storage.search_prompts(&query.q, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search prompts failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

#[derive(Debug, Deserialize)]
pub struct FileSearchQuery {
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

async fn search_by_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FileSearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    state.storage.search_by_file(&query.file_path, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Search by file failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_context_recent(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ContextQuery>,
) -> Result<Json<Vec<Observation>>, StatusCode> {
    state.storage.get_context_for_project(&query.project, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get context recent failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_projects(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<String>>, StatusCode> {
    state.storage.get_all_projects()
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get projects failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<StorageStats>, StatusCode> {
    state.storage.get_stats()
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get stats failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_context_inject(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ContextQuery>,
) -> Result<Json<Vec<Observation>>, StatusCode> {
    state.storage.get_context_for_project(&query.project, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get context inject failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn generate_summary(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionSummaryRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let observations = state.storage.get_session_observations(&req.session_id)
        .map_err(|e| {
            tracing::error!("Get session observations failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let summary = state.llm.generate_session_summary(&observations).await
        .map_err(|e| {
            tracing::error!("Generate summary failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    state.storage.update_session_status_with_summary(&req.session_id, opencode_mem_core::SessionStatus::Completed, Some(&summary))
        .map_err(|e| {
            tracing::error!("Update session summary failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(serde_json::json!({"session_id": req.session_id, "summary": summary})))
}

async fn sse_events(
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
