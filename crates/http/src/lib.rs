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
    Observation, SearchResult, ObservationInput, ToolOutput, ToolCall,
};
use opencode_mem_storage::Storage;
use opencode_mem_llm::LlmClient;

#[derive(Debug, Serialize, Deserialize)]
pub struct ObserveResponse {
    pub id: String,
    pub queued: bool,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
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
pub struct SessionSummaryRequest {
    pub session_id: String,
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
        .route("/observe", post(observe))
        .route("/search", get(search))
        .route("/hybrid-search", get(hybrid_search))
        .route("/observations/:id", get(get_observation))
        .route("/recent", get(get_recent))
        .route("/timeline", get(get_timeline))
        .route("/session/summary", post(generate_summary))
        .route("/events", get(sse_events))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
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
    let observation = state.llm.compress_to_observation(id, &input).await?;
    state.storage.save_observation(&observation)?;
    tracing::info!("Saved observation: {} - {}", observation.id, observation.title);
    let _ = state.event_tx.send(serde_json::to_string(&observation)?);
    Ok(())
}

async fn search(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    if query.q.is_empty() {
        return state.storage.get_recent(query.limit)
            .map(Json)
            .map_err(|e| {
                tracing::error!("Recent observations failed: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            });
    }
    state.storage.search(&query.q, query.limit)
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
    state.storage.update_session_summary(&req.session_id, &summary)
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
