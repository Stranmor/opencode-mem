//! HTTP API server (Axum)
mod viewer;

use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    routing::{delete, get, post},
    Json, Router,
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock, Semaphore};
use tower_http::cors::CorsLayer;

use opencode_mem_core::{
    Observation, ObservationInput, SearchResult, Session, SessionStatus, SessionSummary, ToolCall,
    ToolOutput, UserPrompt,
};
use opencode_mem_infinite::{InfiniteMemory, tool_event};
use opencode_mem_llm::LlmClient;
use opencode_mem_storage::{
    PaginatedResult, PendingMessage, QueueStats, Storage, StorageStats,
    DEFAULT_VISIBILITY_TIMEOUT_SECS,
};

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
pub struct SessionInitRequest {
    #[serde(rename = "contentSessionId")]
    pub content_session_id: Option<String>,
    pub project: Option<String>,
    #[serde(rename = "userPrompt")]
    pub user_prompt: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionInitResponse {
    pub session_id: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionObservationsRequest {
    #[serde(rename = "contentSessionId")]
    pub content_session_id: Option<String>,
    pub observations: Vec<ToolCall>,
}

#[derive(Debug, Serialize)]
pub struct SessionObservationsResponse {
    pub queued: usize,
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionSummarizeRequest {
    #[serde(rename = "contentSessionId")]
    pub content_session_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionStatusResponse {
    pub session_id: String,
    pub status: SessionStatus,
    pub observation_count: usize,
    pub started_at: String,
    pub ended_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionDeleteResponse {
    pub deleted: bool,
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct SessionCompleteResponse {
    pub session_id: String,
    pub status: SessionStatus,
    pub summary: Option<String>,
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

/// In-memory settings storage
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub mcp_enabled: bool,
    #[serde(default)]
    pub current_branch: String,
    #[serde(default)]
    pub log_path: Option<String>,
}

pub struct AppState {
    pub storage: Storage,
    pub llm: LlmClient,
    pub semaphore: Arc<Semaphore>,
    pub event_tx: broadcast::Sender<String>,
    pub processing_active: AtomicBool,
    pub settings: RwLock<Settings>,
    pub infinite_mem: Option<Arc<InfiniteMemory>>,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(viewer::serve_viewer))
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
        .route("/sessions/{sessionDbId}/init", post(session_init_legacy))
        .route(
            "/sessions/{sessionDbId}/observations",
            post(session_observations_legacy),
        )
        .route(
            "/sessions/{sessionDbId}/summarize",
            post(session_summarize_legacy),
        )
        .route("/sessions/{sessionDbId}/status", get(session_status))
        .route("/sessions/{sessionDbId}", delete(session_delete))
        .route("/sessions/{sessionDbId}/complete", post(session_complete))
        .route("/api/sessions/init", post(api_session_init))
        .route("/api/sessions/observations", post(api_session_observations))
        .route("/api/sessions/summarize", post(api_session_summarize))
        // Pending queue management
        .route("/api/pending-queue", get(get_pending_queue))
        .route("/api/pending-queue/process", post(process_pending_queue))
        .route("/api/pending-queue/failed", delete(clear_failed_queue))
        .route("/api/pending-queue/all", delete(clear_all_queue))
        .route("/api/processing-status", get(get_processing_status))
        .route("/api/processing", post(set_processing_status))
        // Settings and MCP management
        .route("/api/settings", get(get_settings))
        .route("/api/settings", post(update_settings))
        .route("/api/mcp/status", get(get_mcp_status))
        .route("/api/mcp/toggle", post(toggle_mcp))
        .route("/api/branch/status", get(get_branch_status))
        .route("/api/branch/switch", post(switch_branch))
        .route("/api/branch/update", post(update_branch))
        // Instructions and admin
        .route("/api/instructions", get(get_instructions))
        .route("/api/admin/restart", post(admin_restart))
        .route("/api/admin/shutdown", post(admin_shutdown))
        // Logs routes
        .route("/api/logs", get(get_logs))
        .route("/api/logs/clear", post(clear_logs))
        .route("/api/unified-search", get(unified_search))
        .route("/api/unified-timeline", get(unified_timeline))
        .route("/api/decisions", get(get_decisions))
        .route("/api/changes", get(get_changes))
        .route("/api/how-it-works", get(get_how_it_works))
        .route("/api/context/timeline", get(context_timeline))
        .route("/api/context/preview", get(context_preview))
        .route("/api/timeline/by-query", get(timeline_by_query))
        .route("/api/search/help", get(search_help))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn readiness() -> (StatusCode, Json<ReadinessResponse>) {
    (
        StatusCode::OK,
        Json(ReadinessResponse {
            status: "ready",
            message: None,
        }),
    )
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

async fn process_observation(
    state: &AppState,
    id: &str,
    tool_call: ToolCall,
) -> anyhow::Result<()> {
    let input = ObservationInput {
        tool: tool_call.tool.clone(),
        session_id: tool_call.session_id.clone(),
        call_id: tool_call.call_id.clone(),
        output: ToolOutput {
            title: format!("Observation from {}", tool_call.tool),
            output: tool_call.output.clone(),
            metadata: tool_call.input.clone(),
        },
    };
    let observation = state
        .llm
        .compress_to_observation(id, &input, tool_call.project.as_deref())
        .await?;
    state.storage.save_observation(&observation)?;
    tracing::info!(
        "Saved observation: {} - {}",
        observation.id,
        observation.title
    );
    let _ = state.event_tx.send(serde_json::to_string(&observation)?);
    
    // Store in infinite memory (PostgreSQL + pgvector)
    if let Some(ref infinite_mem) = state.infinite_mem {
        let event = tool_event(
            &tool_call.session_id,
            tool_call.project.as_deref(),
            &tool_call.tool,
            tool_call.input,
            serde_json::json!({"output": tool_call.output}),
            observation.files_modified.clone(),
        );
        if let Err(e) = infinite_mem.store_event(event).await {
            tracing::warn!("Failed to store in infinite memory: {}", e);
        }
    }
    
    Ok(())
}

async fn search(
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

async fn hybrid_search(
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

async fn get_observation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<Observation>>, StatusCode> {
    state.storage.get_by_id(&id).map(Json).map_err(|e| {
        tracing::error!("Get observation failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

async fn get_recent(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    state
        .storage
        .get_recent(query.limit)
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
    state
        .storage
        .get_timeline(query.from.as_deref(), query.to.as_deref(), query.limit)
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
    state
        .storage
        .get_observations_by_ids(&req.ids)
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
    state
        .storage
        .get_observations_paginated(query.offset, limit, query.project.as_deref())
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
    state
        .storage
        .get_summaries_paginated(query.offset, limit, query.project.as_deref())
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
    state
        .storage
        .get_prompts_paginated(query.offset, limit, query.project.as_deref())
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
    state
        .storage
        .get_session_summary(&id)
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
    state.storage.get_prompt_by_id(&id).map(Json).map_err(|e| {
        tracing::error!("Get prompt by id failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

async fn search_observations(
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

async fn search_by_type(
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

async fn search_by_concept(
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

async fn search_sessions(
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

async fn search_prompts(
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

#[derive(Debug, Deserialize)]
pub struct FileSearchQuery {
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct UnifiedTimelineQuery {
    pub anchor: Option<String>,
    pub q: Option<String>,
    #[serde(default = "default_timeline_count")]
    pub before: usize,
    #[serde(default = "default_timeline_count")]
    pub after: usize,
    pub project: Option<String>,
}

fn default_timeline_count() -> usize {
    5
}

#[derive(Debug, Deserialize)]
pub struct ContextPreviewQuery {
    pub project: String,
    #[serde(default = "default_context_limit")]
    pub limit: usize,
    #[serde(default = "default_preview_format")]
    pub format: String,
}

fn default_preview_format() -> String {
    "compact".to_string()
}

#[derive(Debug, Serialize)]
pub struct UnifiedSearchResult {
    pub observations: Vec<SearchResult>,
    pub sessions: Vec<SessionSummary>,
    pub prompts: Vec<UserPrompt>,
}

#[derive(Debug, Serialize)]
pub struct TimelineResult {
    pub anchor: Option<SearchResult>,
    pub before: Vec<SearchResult>,
    pub after: Vec<SearchResult>,
}

#[derive(Debug, Serialize)]
pub struct ContextPreview {
    pub project: String,
    pub observation_count: usize,
    pub preview: String,
}

#[derive(Debug, Serialize)]
pub struct SearchHelpResponse {
    pub endpoints: Vec<EndpointDoc>,
}

#[derive(Debug, Serialize)]
pub struct EndpointDoc {
    pub path: &'static str,
    pub method: &'static str,
    pub description: &'static str,
    pub params: Vec<ParamDoc>,
}

#[derive(Debug, Serialize)]
pub struct ParamDoc {
    pub name: &'static str,
    pub required: bool,
    pub description: &'static str,
}

async fn search_by_file(
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

async fn get_context_recent(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ContextQuery>,
) -> Result<Json<Vec<Observation>>, StatusCode> {
    state
        .storage
        .get_context_for_project(&query.project, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get context recent failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_projects(State(state): State<Arc<AppState>>) -> Result<Json<Vec<String>>, StatusCode> {
    state.storage.get_all_projects().map(Json).map_err(|e| {
        tracing::error!("Get projects failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

async fn get_stats(State(state): State<Arc<AppState>>) -> Result<Json<StorageStats>, StatusCode> {
    state.storage.get_stats().map(Json).map_err(|e| {
        tracing::error!("Get stats failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

async fn get_context_inject(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ContextQuery>,
) -> Result<Json<Vec<Observation>>, StatusCode> {
    state
        .storage
        .get_context_for_project(&query.project, query.limit)
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
    let observations = state
        .storage
        .get_session_observations(&req.session_id)
        .map_err(|e| {
            tracing::error!("Get session observations failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let summary = state
        .llm
        .generate_session_summary(&observations)
        .await
        .map_err(|e| {
            tracing::error!("Generate summary failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    state
        .storage
        .update_session_status_with_summary(
            &req.session_id,
            opencode_mem_core::SessionStatus::Completed,
            Some(&summary),
        )
        .map_err(|e| {
            tracing::error!("Update session summary failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(
        serde_json::json!({"session_id": req.session_id, "summary": summary}),
    ))
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

async fn session_init_legacy(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
    Json(req): Json<SessionInitRequest>,
) -> Result<Json<SessionInitResponse>, StatusCode> {
    let session = Session {
        id: session_db_id.clone(),
        content_session_id: req
            .content_session_id
            .unwrap_or_else(|| session_db_id.clone()),
        memory_session_id: None,
        project: req.project.unwrap_or_default(),
        user_prompt: req.user_prompt,
        started_at: chrono::Utc::now(),
        ended_at: None,
        status: SessionStatus::Active,
        prompt_counter: 0,
    };
    state.storage.save_session(&session).map_err(|e| {
        tracing::error!("Session init failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(SessionInitResponse {
        session_id: session.id,
        status: "active".to_string(),
    }))
}

async fn session_observations_legacy(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
    Json(req): Json<SessionObservationsRequest>,
) -> Result<Json<SessionObservationsResponse>, StatusCode> {
    let count = req.observations.len();
    for tool_call in req.observations {
        let id = uuid::Uuid::new_v4().to_string();
        let state_clone = state.clone();
        let session_id = session_db_id.clone();
        tokio::spawn(async move {
            let permit = match state_clone.semaphore.acquire().await {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Semaphore closed: {}", e);
                    return;
                }
            };
            if let Err(e) =
                process_observation_for_session(&state_clone, &id, &session_id, tool_call).await
            {
                tracing::error!("Failed to process observation: {}", e);
            }
            drop(permit);
        });
    }
    Ok(Json(SessionObservationsResponse {
        queued: count,
        session_id: session_db_id,
    }))
}

async fn process_observation_for_session(
    state: &AppState,
    id: &str,
    session_id: &str,
    tool_call: ToolCall,
) -> anyhow::Result<()> {
    let input = ObservationInput {
        tool: tool_call.tool.clone(),
        session_id: session_id.to_string(),
        call_id: tool_call.call_id.clone(),
        output: ToolOutput {
            title: format!("Observation from {}", tool_call.tool),
            output: tool_call.output.clone(),
            metadata: tool_call.input.clone(),
        },
    };
    let observation = state
        .llm
        .compress_to_observation(id, &input, tool_call.project.as_deref())
        .await?;
    state.storage.save_observation(&observation)?;
    tracing::info!(
        "Saved observation: {} - {}",
        observation.id,
        observation.title
    );
    let _ = state.event_tx.send(serde_json::to_string(&observation)?);
    
    if let Some(ref infinite_mem) = state.infinite_mem {
        let event = tool_event(
            session_id,
            tool_call.project.as_deref(),
            &tool_call.tool,
            tool_call.input,
            serde_json::json!({"output": tool_call.output}),
            observation.files_modified.clone(),
        );
        if let Err(e) = infinite_mem.store_event(event).await {
            tracing::warn!("Failed to store in infinite memory: {}", e);
        }
    }
    
    Ok(())
}

async fn session_summarize_legacy(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let observations = state
        .storage
        .get_session_observations(&session_db_id)
        .map_err(|e| {
            tracing::error!("Get session observations failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let summary = state
        .llm
        .generate_session_summary(&observations)
        .await
        .map_err(|e| {
            tracing::error!("Generate summary failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    state
        .storage
        .update_session_status_with_summary(
            &session_db_id,
            SessionStatus::Completed,
            Some(&summary),
        )
        .map_err(|e| {
            tracing::error!("Update session summary failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(
        serde_json::json!({"session_id": session_db_id, "summary": summary, "queued": true}),
    ))
}

async fn session_status(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<SessionStatusResponse>, StatusCode> {
    let session = state.storage.get_session(&session_db_id).map_err(|e| {
        tracing::error!("Get session failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match session {
        Some(s) => {
            let obs_count = state
                .storage
                .get_session_observation_count(&session_db_id)
                .unwrap_or(0);
            Ok(Json(SessionStatusResponse {
                session_id: s.id,
                status: s.status,
                observation_count: obs_count,
                started_at: s.started_at.to_rfc3339(),
                ended_at: s.ended_at.map(|d| d.to_rfc3339()),
            }))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn session_delete(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<SessionDeleteResponse>, StatusCode> {
    let deleted = state.storage.delete_session(&session_db_id).map_err(|e| {
        tracing::error!("Delete session failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(SessionDeleteResponse {
        deleted,
        session_id: session_db_id,
    }))
}

async fn session_complete(
    State(state): State<Arc<AppState>>,
    Path(session_db_id): Path<String>,
) -> Result<Json<SessionCompleteResponse>, StatusCode> {
    let observations = state
        .storage
        .get_session_observations(&session_db_id)
        .map_err(|e| {
            tracing::error!("Get session observations failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let summary = if observations.is_empty() {
        None
    } else {
        Some(
            state
                .llm
                .generate_session_summary(&observations)
                .await
                .map_err(|e| {
                    tracing::error!("Generate summary failed: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                })?,
        )
    };
    state
        .storage
        .update_session_status_with_summary(
            &session_db_id,
            SessionStatus::Completed,
            summary.as_deref(),
        )
        .map_err(|e| {
            tracing::error!("Update session status failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(SessionCompleteResponse {
        session_id: session_db_id,
        status: SessionStatus::Completed,
        summary,
    }))
}

async fn api_session_init(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionInitRequest>,
) -> Result<Json<SessionInitResponse>, StatusCode> {
    let content_session_id = req.content_session_id.ok_or(StatusCode::BAD_REQUEST)?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let session = Session {
        id: session_id.clone(),
        content_session_id,
        memory_session_id: None,
        project: req.project.unwrap_or_default(),
        user_prompt: req.user_prompt,
        started_at: chrono::Utc::now(),
        ended_at: None,
        status: SessionStatus::Active,
        prompt_counter: 0,
    };
    state.storage.save_session(&session).map_err(|e| {
        tracing::error!("API session init failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(SessionInitResponse {
        session_id: session.id,
        status: "active".to_string(),
    }))
}

async fn api_session_observations(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionObservationsRequest>,
) -> Result<Json<SessionObservationsResponse>, StatusCode> {
    let content_session_id = req.content_session_id.ok_or(StatusCode::BAD_REQUEST)?;
    let session = state
        .storage
        .get_session_by_content_id(&content_session_id)
        .map_err(|e| {
            tracing::error!("Get session by content id failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let session_id = session.map(|s| s.id).ok_or(StatusCode::NOT_FOUND)?;
    let count = req.observations.len();
    for tool_call in req.observations {
        let id = uuid::Uuid::new_v4().to_string();
        let state_clone = state.clone();
        let sid = session_id.clone();
        tokio::spawn(async move {
            let permit = match state_clone.semaphore.acquire().await {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Semaphore closed: {}", e);
                    return;
                }
            };
            if let Err(e) =
                process_observation_for_session(&state_clone, &id, &sid, tool_call).await
            {
                tracing::error!("Failed to process observation: {}", e);
            }
            drop(permit);
        });
    }
    Ok(Json(SessionObservationsResponse {
        queued: count,
        session_id,
    }))
}

async fn api_session_summarize(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionSummarizeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let content_session_id = req.content_session_id.ok_or(StatusCode::BAD_REQUEST)?;
    let session = state
        .storage
        .get_session_by_content_id(&content_session_id)
        .map_err(|e| {
            tracing::error!("Get session by content id failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let session_id = session.map(|s| s.id).ok_or(StatusCode::NOT_FOUND)?;
    let observations = state
        .storage
        .get_session_observations(&session_id)
        .map_err(|e| {
            tracing::error!("Get session observations failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let summary = state
        .llm
        .generate_session_summary(&observations)
        .await
        .map_err(|e| {
            tracing::error!("Generate summary failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    state
        .storage
        .update_session_status_with_summary(&session_id, SessionStatus::Completed, Some(&summary))
        .map_err(|e| {
            tracing::error!("Update session summary failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(
        serde_json::json!({"session_id": session_id, "summary": summary, "queued": true}),
    ))
}

#[derive(Debug, Serialize)]
pub struct PendingQueueResponse {
    pub messages: Vec<PendingMessage>,
    pub stats: QueueStats,
}

#[derive(Debug, Serialize)]
pub struct ProcessQueueResponse {
    pub processed: usize,
    pub failed: usize,
}

#[derive(Debug, Serialize)]
pub struct ClearQueueResponse {
    pub cleared: usize,
}

#[derive(Debug, Serialize)]
pub struct ProcessingStatusResponse {
    pub active: bool,
    pub pending_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct SetProcessingRequest {
    pub active: bool,
}

#[derive(Debug, Serialize)]
pub struct SetProcessingResponse {
    pub active: bool,
}

async fn get_pending_queue(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<PendingQueueResponse>, StatusCode> {
    let messages = state
        .storage
        .get_all_pending_messages(query.limit)
        .map_err(|e| {
            tracing::error!("Get pending queue failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let stats = state.storage.get_queue_stats().map_err(|e| {
        tracing::error!("Get queue stats failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(PendingQueueResponse { messages, stats }))
}

async fn process_pending_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProcessQueueResponse>, StatusCode> {
    let messages = state
        .storage
        .claim_pending_messages(10, DEFAULT_VISIBILITY_TIMEOUT_SECS)
        .map_err(|e| {
            tracing::error!("Claim pending messages failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let count = messages.len();
    for msg in messages {
        let state_clone = state.clone();
        tokio::spawn(async move {
            let permit = match state_clone.semaphore.acquire().await {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Semaphore closed: {}", e);
                    return;
                }
            };
            let result = process_pending_message(&state_clone, &msg).await;
            match result {
                Ok(()) => {
                    if let Err(e) = state_clone.storage.complete_message(msg.id) {
                        tracing::error!("Complete message {} failed: {}", msg.id, e);
                    }
                }
                Err(e) => {
                    tracing::error!("Process message {} failed: {}", msg.id, e);
                    let _ = state_clone.storage.fail_message(msg.id, true);
                }
            }
            drop(permit);
        });
    }
    Ok(Json(ProcessQueueResponse {
        processed: count,
        failed: 0,
    }))
}

async fn process_pending_message(state: &AppState, msg: &PendingMessage) -> anyhow::Result<()> {
    use opencode_mem_core::{ObservationInput, ToolOutput};

    let tool_name = msg.tool_name.as_deref().unwrap_or("unknown");
    let tool_input = msg.tool_input.clone();
    let tool_response = msg.tool_response.as_deref().unwrap_or("");

    let input = ObservationInput {
        tool: tool_name.to_string(),
        session_id: msg.session_id.clone(),
        call_id: String::new(),
        output: ToolOutput {
            title: format!("Observation from {}", tool_name),
            output: tool_response.to_string(),
            metadata: tool_input
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(serde_json::Value::Null),
        },
    };

    let id = uuid::Uuid::new_v4().to_string();
    let observation = state.llm.compress_to_observation(&id, &input, None).await?;
    state.storage.save_observation(&observation)?;
    tracing::info!(
        "Processed pending message {} -> observation {}",
        msg.id,
        observation.id
    );
    let _ = state.event_tx.send(serde_json::to_string(&observation)?);
    
    if let Some(ref infinite_mem) = state.infinite_mem {
        let event = tool_event(
            &msg.session_id,
            None,
            tool_name,
            tool_input.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(serde_json::Value::Null),
            serde_json::json!({"output": tool_response}),
            observation.files_modified.clone(),
        );
        if let Err(e) = infinite_mem.store_event(event).await {
            tracing::warn!("Failed to store in infinite memory: {}", e);
        }
    }
    
    Ok(())
}

async fn clear_failed_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClearQueueResponse>, StatusCode> {
    let cleared = state.storage.clear_failed_messages().map_err(|e| {
        tracing::error!("Clear failed messages failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(ClearQueueResponse { cleared }))
}

async fn clear_all_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClearQueueResponse>, StatusCode> {
    let cleared = state.storage.clear_all_pending_messages().map_err(|e| {
        tracing::error!("Clear all pending messages failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(ClearQueueResponse { cleared }))
}

async fn get_processing_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProcessingStatusResponse>, StatusCode> {
    let active = state.processing_active.load(Ordering::SeqCst);
    let pending_count = state.storage.get_pending_count().map_err(|e| {
        tracing::error!("Get pending count failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(ProcessingStatusResponse {
        active,
        pending_count,
    }))
}

async fn set_processing_status(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetProcessingRequest>,
) -> Result<Json<SetProcessingResponse>, StatusCode> {
    state.processing_active.store(req.active, Ordering::SeqCst);
    Ok(Json(SetProcessingResponse { active: req.active }))
}

pub fn run_startup_recovery(state: &AppState) -> anyhow::Result<usize> {
    let released = state
        .storage
        .release_stale_messages(DEFAULT_VISIBILITY_TIMEOUT_SECS)?;
    if released > 0 {
        tracing::info!(
            "Startup recovery: released {} stale messages back to pending",
            released
        );
    }
    Ok(released)
}

#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    pub settings: Settings,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSettingsRequest {
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    pub log_path: Option<String>,
}

async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SettingsResponse>, StatusCode> {
    let settings = state.settings.read().await.clone();
    Ok(Json(SettingsResponse { settings }))
}

async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<SettingsResponse>, StatusCode> {
    let mut settings = state.settings.write().await;
    if let Some(env) = req.env {
        settings.env = env;
    }
    Ok(Json(SettingsResponse {
        settings: settings.clone(),
    }))
}

#[derive(Debug, Serialize)]
pub struct McpStatusResponse {
    pub enabled: bool,
}

async fn get_mcp_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<McpStatusResponse>, StatusCode> {
    let settings = state.settings.read().await;
    Ok(Json(McpStatusResponse {
        enabled: settings.mcp_enabled,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ToggleMcpRequest {
    pub enabled: bool,
}

async fn toggle_mcp(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ToggleMcpRequest>,
) -> Result<Json<McpStatusResponse>, StatusCode> {
    let mut settings = state.settings.write().await;
    settings.mcp_enabled = req.enabled;
    Ok(Json(McpStatusResponse {
        enabled: settings.mcp_enabled,
    }))
}

#[derive(Debug, Serialize)]
pub struct BranchStatusResponse {
    pub current_branch: String,
    pub is_dirty: bool,
}

async fn get_branch_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<BranchStatusResponse>, StatusCode> {
    let settings = state.settings.read().await;
    Ok(Json(BranchStatusResponse {
        current_branch: settings.current_branch.clone(),
        is_dirty: false,
    }))
}

#[derive(Debug, Deserialize)]
pub struct SwitchBranchRequest {
    pub branch: String,
}

#[derive(Debug, Serialize)]
pub struct SwitchBranchResponse {
    pub success: bool,
    pub branch: String,
    pub message: String,
}

async fn switch_branch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SwitchBranchRequest>,
) -> Result<Json<SwitchBranchResponse>, StatusCode> {
    let mut settings = state.settings.write().await;
    settings.current_branch = req.branch.clone();
    Ok(Json(SwitchBranchResponse {
        success: true,
        branch: req.branch,
        message: "Branch switch stubbed".to_string(),
    }))
}

#[derive(Debug, Serialize)]
pub struct UpdateBranchResponse {
    pub success: bool,
    pub message: String,
}

async fn update_branch(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<UpdateBranchResponse>, StatusCode> {
    Ok(Json(UpdateBranchResponse {
        success: true,
        message: "Branch update stubbed".to_string(),
    }))
}

#[derive(Debug, Serialize)]
pub struct InstructionsResponse {
    pub sections: Vec<String>,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct InstructionsQuery {
    #[serde(default)]
    pub section: Option<String>,
}

async fn get_instructions(
    Query(query): Query<InstructionsQuery>,
) -> Result<Json<InstructionsResponse>, StatusCode> {
    let content = tokio::task::spawn_blocking(|| {
        let skill_path = std::path::Path::new("SKILL.md");
        if skill_path.exists() {
            std::fs::read_to_string(skill_path).unwrap_or_default()
        } else {
            String::new()
        }
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let sections: Vec<String> = content
        .lines()
        .filter(|l| l.starts_with("## "))
        .map(|l| l.trim_start_matches("## ").to_string())
        .collect();
    let filtered_content = if let Some(section) = query.section {
        extract_section(&content, &section)
    } else {
        content
    };
    Ok(Json(InstructionsResponse {
        sections,
        content: filtered_content,
    }))
}

fn extract_section(content: &str, section: &str) -> String {
    let marker = format!("## {}", section);
    let mut in_section = false;
    let mut result = Vec::new();
    for line in content.lines() {
        if line.starts_with("## ") {
            if line == marker {
                in_section = true;
                result.push(line);
            } else if in_section {
                break;
            }
        } else if in_section {
            result.push(line);
        }
    }
    result.join("\n")
}

fn is_localhost(addr: &SocketAddr) -> bool {
    addr.ip().is_loopback()
}

#[derive(Debug, Serialize)]
pub struct AdminResponse {
    pub success: bool,
    pub message: String,
}

async fn admin_restart(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<AdminResponse>, StatusCode> {
    if !is_localhost(&addr) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(Json(AdminResponse {
        success: true,
        message: "Restart signal sent (stubbed)".to_string(),
    }))
}

async fn admin_shutdown(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<AdminResponse>, StatusCode> {
    if !is_localhost(&addr) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(Json(AdminResponse {
        success: true,
        message: "Shutdown signal sent (stubbed)".to_string(),
    }))
}

#[derive(Debug, Serialize)]
pub struct LogsResponse {
    pub date: String,
    pub content: String,
    pub size_bytes: usize,
}

#[allow(dead_code)]
async fn get_logs(State(state): State<Arc<AppState>>) -> Result<Json<LogsResponse>, StatusCode> {
    const MAX_BYTES: usize = 512 * 1024; // 512KB max
    
    let settings = state.settings.read().await;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let log_path = settings.log_path.clone();
    drop(settings);
    let content = if let Some(log_path) = log_path {
        tokio::task::spawn_blocking(move || {
            let path = std::path::Path::new(&log_path);
            if !path.exists() {
                return String::new();
            }
            let metadata = std::fs::metadata(path).ok();
            let file_size = metadata.map(|m| m.len() as usize).unwrap_or(0);
            
            if file_size <= MAX_BYTES {
                std::fs::read_to_string(path).unwrap_or_default()
            } else {
                use std::io::{Read, Seek, SeekFrom};
                let mut file = match std::fs::File::open(path) {
                    Ok(f) => f,
                    Err(_) => return String::new(),
                };
                let skip = file_size.saturating_sub(MAX_BYTES);
                if file.seek(SeekFrom::Start(skip as u64)).is_err() {
                    return String::new();
                }
                let mut buf = String::with_capacity(MAX_BYTES);
                if file.read_to_string(&mut buf).is_err() {
                    return String::new();
                }
                if let Some(newline_pos) = buf.find('\n') {
                    buf = buf[newline_pos + 1..].to_string();
                }
                format!("... (truncated, showing last ~500KB) ...\n{}", buf)
            }
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        String::new()
    };
    let size_bytes = content.len();
    Ok(Json(LogsResponse {
        date: today,
        content,
        size_bytes,
    }))
}

#[derive(Debug, Serialize)]
pub struct ClearLogsResponse {
    pub success: bool,
    pub message: String,
}

#[allow(dead_code)]
async fn clear_logs(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClearLogsResponse>, StatusCode> {
    let settings = state.settings.read().await;
    let log_path = settings.log_path.clone();
    drop(settings);
    if let Some(log_path) = log_path {
        let result = tokio::task::spawn_blocking(move || {
            let path = std::path::Path::new(&log_path);
            if path.exists() {
                std::fs::write(path, "")
            } else {
                Ok(())
            }
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Err(e) = result {
            tracing::error!("Failed to clear logs: {}", e);
            return Ok(Json(ClearLogsResponse {
                success: false,
                message: format!("Failed to clear: {}", e),
            }));
        }
    }
    Ok(Json(ClearLogsResponse {
        success: true,
        message: "Logs cleared".to_string(),
    }))
}

async fn unified_search(
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

async fn unified_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UnifiedTimelineQuery>,
) -> Result<Json<TimelineResult>, StatusCode> {
    let anchor_sr = if let Some(id) = &query.anchor {
        state
            .storage
            .get_by_id(id)
            .ok()
            .flatten()
            .map(|obs| SearchResult {
                id: obs.id,
                title: obs.title,
                subtitle: obs.subtitle.clone(),
                observation_type: obs.observation_type,
                score: 1.0,
            })
    } else if let Some(q) = &query.q {
        state
            .storage
            .hybrid_search(q, 1)
            .ok()
            .and_then(|r| r.into_iter().next())
    } else {
        None
    };
    let (before, after) = if let Some(ref anchor) = anchor_sr {
        let all = state
            .storage
            .get_timeline(None, None, query.before + query.after + 50)
            .unwrap_or_default();
        match all.iter().position(|o| o.id == anchor.id) {
            Some(pos) => {
                let before_items: Vec<_> = all[..pos]
                    .iter()
                    .rev()
                    .take(query.before)
                    .cloned()
                    .collect();
                let after_items: Vec<_> = all
                    .get(pos + 1..)
                    .unwrap_or(&[])
                    .iter()
                    .take(query.after)
                    .cloned()
                    .collect();
                (before_items, after_items)
            }
            None => {
                // Anchor not in recent timeline - return empty context
                (Vec::new(), Vec::new())
            }
        }
    } else {
        (Vec::new(), Vec::new())
    };
    Ok(Json(TimelineResult {
        anchor: anchor_sr,
        before,
        after,
    }))
}

async fn get_decisions(
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
        .search_with_filters(q, query.project.as_deref(), Some("decision"), query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get decisions failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_changes(
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
        .search_with_filters(q, query.project.as_deref(), Some("change"), query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get changes failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn get_how_it_works(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let search_query = if query.q.is_empty() {
        "how-it-works".to_string()
    } else {
        format!("{} how-it-works", query.q)
    };
    state
        .storage
        .hybrid_search(&search_query, query.limit)
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get how-it-works failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

async fn context_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UnifiedTimelineQuery>,
) -> Result<Json<TimelineResult>, StatusCode> {
    unified_timeline(State(state), Query(query)).await
}

async fn context_preview(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ContextPreviewQuery>,
) -> Result<Json<ContextPreview>, StatusCode> {
    let observations = state
        .storage
        .get_context_for_project(&query.project, query.limit)
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

async fn timeline_by_query(
    State(state): State<Arc<AppState>>,
    Query(query): Query<UnifiedTimelineQuery>,
) -> Result<Json<TimelineResult>, StatusCode> {
    unified_timeline(State(state), Query(query)).await
}

async fn search_help() -> Json<SearchHelpResponse> {
    Json(SearchHelpResponse {
        endpoints: vec![
            EndpointDoc {
                path: "/api/unified-search",
                method: "GET",
                description: "Unified search across observations, sessions, and prompts",
                params: vec![
                    ParamDoc {
                        name: "q",
                        required: true,
                        description: "Search query",
                    },
                    ParamDoc {
                        name: "limit",
                        required: false,
                        description: "Max results (default 20)",
                    },
                    ParamDoc {
                        name: "project",
                        required: false,
                        description: "Filter by project",
                    },
                    ParamDoc {
                        name: "type",
                        required: false,
                        description: "Filter by observation type",
                    },
                ],
            },
            EndpointDoc {
                path: "/api/unified-timeline",
                method: "GET",
                description: "Get timeline centered around an anchor observation",
                params: vec![
                    ParamDoc {
                        name: "anchor",
                        required: false,
                        description: "Observation ID to center on",
                    },
                    ParamDoc {
                        name: "q",
                        required: false,
                        description: "Search query to find anchor",
                    },
                    ParamDoc {
                        name: "before",
                        required: false,
                        description: "Count before anchor (default 5)",
                    },
                    ParamDoc {
                        name: "after",
                        required: false,
                        description: "Count after anchor (default 5)",
                    },
                ],
            },
            EndpointDoc {
                path: "/api/decisions",
                method: "GET",
                description: "Get observations of type 'decision'",
                params: vec![
                    ParamDoc {
                        name: "q",
                        required: false,
                        description: "Optional search filter",
                    },
                    ParamDoc {
                        name: "limit",
                        required: false,
                        description: "Max results",
                    },
                ],
            },
            EndpointDoc {
                path: "/api/changes",
                method: "GET",
                description: "Get observations of type 'change'",
                params: vec![
                    ParamDoc {
                        name: "q",
                        required: false,
                        description: "Optional search filter",
                    },
                    ParamDoc {
                        name: "limit",
                        required: false,
                        description: "Max results",
                    },
                ],
            },
            EndpointDoc {
                path: "/api/how-it-works",
                method: "GET",
                description: "Search for 'how-it-works' concept observations",
                params: vec![
                    ParamDoc {
                        name: "q",
                        required: false,
                        description: "Additional search terms",
                    },
                    ParamDoc {
                        name: "limit",
                        required: false,
                        description: "Max results",
                    },
                ],
            },
            EndpointDoc {
                path: "/api/context/preview",
                method: "GET",
                description: "Generate context preview for a project",
                params: vec![
                    ParamDoc {
                        name: "project",
                        required: true,
                        description: "Project path",
                    },
                    ParamDoc {
                        name: "limit",
                        required: false,
                        description: "Max observations",
                    },
                    ParamDoc {
                        name: "format",
                        required: false,
                        description: "'compact' or 'full'",
                    },
                ],
            },
        ],
    })
}
