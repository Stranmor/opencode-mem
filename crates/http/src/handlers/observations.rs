use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use opencode_mem_embeddings::EmbeddingProvider;
use std::sync::Arc;
use tokio::task::spawn_blocking;

use opencode_mem_core::{
    is_low_value_observation, Observation, ObservationType, ProjectFilter, SearchResult,
    SessionSummary, ToolCall, UserPrompt,
};
use opencode_mem_storage::{
    EmbeddingStore, ObservationStore, PaginatedResult, PendingQueueStore, PromptStore, SearchStore,
    StatsStore, SummaryStore,
};

use crate::api_types::{
    BatchRequest, ObserveBatchResponse, ObserveResponse, PaginationQuery, SaveMemoryRequest,
    SearchQuery, TimelineQuery,
};
use crate::AppState;

pub async fn observe(
    State(state): State<Arc<AppState>>,
    Json(tool_call): Json<ToolCall>,
) -> Result<Json<ObserveResponse>, StatusCode> {
    if let Some(project) = tool_call.project.as_deref() {
        if ProjectFilter::global().is_some_and(|filter| filter.is_excluded(project)) {
            return Ok(Json(ObserveResponse { id: String::new(), queued: false }));
        }
    }

    // Serialize tool_call.input as tool_input for queue processing
    let tool_input = serde_json::to_string(&tool_call.input).ok();
    let session_id = tool_call.session_id.clone();
    let tool_name = tool_call.tool.clone();
    let tool_response = tool_call.output.clone();
    let project = tool_call.project.clone();

    let message_id = state
        .storage
        .queue_message(
            &session_id,
            Some(&tool_name),
            tool_input.as_deref(),
            Some(&tool_response),
            project.as_deref(),
        )
        .await
        .map_err(|e| {
            tracing::error!("Queue message error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(ObserveResponse { id: message_id.to_string(), queued: true }))
}

pub async fn observe_batch(
    State(state): State<Arc<AppState>>,
    Json(tool_calls): Json<Vec<ToolCall>>,
) -> Result<Json<ObserveBatchResponse>, StatusCode> {
    let total = tool_calls.len();
    let mut count = 0usize;
    for tool_call in &tool_calls {
        if let Some(project) = tool_call.project.as_deref() {
            if ProjectFilter::global().is_some_and(|filter| filter.is_excluded(project)) {
                continue;
            }
        }
        let tool_input = serde_json::to_string(&tool_call.input).ok();
        match state
            .storage
            .queue_message(
                &tool_call.session_id,
                Some(&tool_call.tool),
                tool_input.as_deref(),
                Some(&tool_call.output),
                tool_call.project.as_deref(),
            )
            .await
        {
            Ok(_id) => count = count.saturating_add(1),
            Err(e) => {
                tracing::error!("Failed to queue tool call {}: {}", tool_call.tool, e);
            },
        }
    }
    Ok(Json(ObserveBatchResponse { queued: count, total }))
}

pub async fn save_memory(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveMemoryRequest>,
) -> Result<(StatusCode, Json<Observation>), StatusCode> {
    let text = req.text.trim().to_owned();
    if text.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let title = req
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| text.chars().take(50).collect());
    let project =
        req.project.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(ToOwned::to_owned);

    let observation = Observation::builder(
        uuid::Uuid::new_v4().to_string(),
        "manual".to_owned(),
        ObservationType::Discovery,
        title,
    )
    .maybe_project(project)
    .narrative(text)
    .build();

    if is_low_value_observation(&observation.title) {
        tracing::debug!("Filtered low-value save_memory: {}", observation.title);
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, Json(observation)));
    }

    let obs_for_save = observation.clone();
    let inserted = state.storage.save_observation(&obs_for_save).await.map_err(|e| {
        tracing::error!("Save observation error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !inserted {
        tracing::debug!("Skipping duplicate save_memory: {}", observation.title);
        return Ok((StatusCode::CONFLICT, Json(observation)));
    }

    if let Some(emb) = state.embeddings.clone() {
        let embedding_text = format!(
            "{} {} {}",
            observation.title,
            observation.narrative.as_deref().unwrap_or(""),
            observation.facts.join(" ")
        );
        match spawn_blocking(move || emb.embed(&embedding_text)).await {
            Ok(Ok(vec)) => {
                let obs_id = observation.id.clone();
                match state.storage.store_embedding(&obs_id, &vec).await {
                    Ok(()) => {},
                    Err(e) => {
                        tracing::warn!("Failed to store embedding for {}: {}", observation.id, e);
                    },
                }
            },
            Ok(Err(e)) => {
                tracing::warn!("Failed to generate embedding for {}: {}", observation.id, e);
            },
            Err(e) => {
                tracing::warn!("Embedding generation join error for {}: {}", observation.id, e);
            },
        }
    }

    Ok((StatusCode::CREATED, Json(observation)))
}

pub async fn get_observation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<Observation>>, StatusCode> {
    state.storage.get_by_id(&id).await.map(Json).map_err(|e| {
        tracing::error!("Get observation error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn get_recent(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    state.storage.get_recent(query.limit).await.map(Json).map_err(|e| {
        tracing::error!("Get recent error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn get_timeline(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    state
        .storage
        .get_timeline(query.from.as_deref(), query.to.as_deref(), query.limit)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get timeline error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_observations_batch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BatchRequest>,
) -> Result<Json<Vec<Observation>>, StatusCode> {
    state.storage.get_observations_by_ids(&req.ids).await.map(Json).map_err(|e| {
        tracing::error!("Get observations batch error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn get_observations_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<Observation>>, StatusCode> {
    let limit = query.limit.min(100);
    state
        .storage
        .get_observations_paginated(query.offset, limit, query.project.as_deref())
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get observations paginated error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_summaries_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<SessionSummary>>, StatusCode> {
    let limit = query.limit.min(100);
    state
        .storage
        .get_summaries_paginated(query.offset, limit, query.project.as_deref())
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get summaries paginated error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_prompts_paginated(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResult<UserPrompt>>, StatusCode> {
    let limit = query.limit.min(100);
    state
        .storage
        .get_prompts_paginated(query.offset, limit, query.project.as_deref())
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("Get prompts paginated error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn get_session_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<SessionSummary>>, StatusCode> {
    state.storage.get_session_summary(&id).await.map(Json).map_err(|e| {
        tracing::error!("Get session summary error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

pub async fn get_prompt_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Option<UserPrompt>>, StatusCode> {
    state.storage.get_prompt_by_id(&id).await.map(Json).map_err(|e| {
        tracing::error!("Get prompt error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}
