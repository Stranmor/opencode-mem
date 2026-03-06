use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{delete, get, post, put},
};
use std::sync::Arc;

use crate::api_types::{HealthResponse, ReadinessResponse, VersionResponse};
use crate::{AppState, handlers, viewer};

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(viewer::serve_viewer))
        .route("/health", get(health))
        .route("/api/readiness", get(readiness))
        .route("/api/version", get(version))
        .route("/observe", post(handlers::observations::observe))
        .route(
            "/api/observe/batch",
            post(handlers::observations::observe_batch),
        )
        .route(
            "/api/memory/save",
            post(handlers::observations::save_memory),
        )
        .route("/search", get(handlers::search::search))
        .route("/hybrid-search", get(handlers::search::hybrid_search))
        .route("/semantic-search", get(handlers::search::semantic_search))
        .route(
            "/observations/{id}",
            get(handlers::observations::get_observation)
                .delete(handlers::observations::delete_observation),
        )
        .route(
            "/observations/batch",
            post(handlers::observations::get_observations_batch),
        )
        .route(
            "/api/observations",
            get(handlers::observations::get_observations_paginated),
        )
        .route(
            "/api/summaries",
            get(handlers::observations::get_summaries_paginated),
        )
        .route(
            "/api/prompts",
            get(handlers::observations::get_prompts_paginated),
        )
        .route(
            "/api/session/{id}",
            get(handlers::observations::get_session_by_id),
        )
        .route(
            "/api/prompt/{id}",
            get(handlers::observations::get_prompt_by_id),
        )
        .route("/api/search/observations", get(handlers::search::search))
        .route("/api/search/by-type", get(handlers::search::search))
        .route("/api/search/by-concept", get(handlers::search::search))
        .route(
            "/api/search/sessions",
            get(handlers::search::search_sessions),
        )
        .route("/api/search/prompts", get(handlers::search::search_prompts))
        .route("/api/search/by-file", get(handlers::search::search_by_file))
        .route(
            "/api/context/recent",
            get(handlers::context::get_context_recent),
        )
        .route("/recent", get(handlers::observations::get_recent))
        .route("/timeline", get(handlers::observations::get_timeline))
        .route("/projects", get(handlers::context::get_projects))
        .route("/stats", get(handlers::context::get_stats))
        .route(
            "/context/inject",
            get(handlers::context::get_context_recent),
        )
        .route(
            "/session/summary",
            post(handlers::sessions::generate_summary),
        )
        .route("/events", get(handlers::context::sse_events))
        .route(
            "/sessions/{sessionDbId}/init",
            post(handlers::sessions::session_init_legacy),
        )
        .route(
            "/sessions/{sessionDbId}/observations",
            post(handlers::sessions::session_observations_legacy),
        )
        .route(
            "/sessions/{sessionDbId}/summarize",
            post(handlers::sessions::session_summarize_legacy),
        )
        .route(
            "/sessions/{sessionDbId}/status",
            get(handlers::sessions::session_status),
        )
        .route(
            "/sessions/{sessionDbId}",
            delete(handlers::sessions::session_delete),
        )
        .route(
            "/sessions/{sessionDbId}/complete",
            post(handlers::sessions::session_complete),
        )
        .route(
            "/api/sessions/init",
            post(handlers::sessions_api::api_session_init),
        )
        .route(
            "/api/sessions/observations",
            post(handlers::sessions_api::api_session_observations),
        )
        .route(
            "/api/sessions/summarize",
            post(handlers::sessions_api::api_session_summarize),
        )
        .route(
            "/api/pending-queue",
            get(handlers::queue::get_pending_queue),
        )
        .route(
            "/api/pending-queue/process",
            post(handlers::queue::process_pending_queue),
        )
        .route(
            "/api/pending-queue/failed",
            delete(handlers::queue::clear_failed_queue),
        )
        .route(
            "/api/pending-queue/retry-failed",
            post(handlers::queue::retry_failed_queue),
        )
        .route(
            "/api/pending-queue/all",
            delete(handlers::queue::clear_all_queue),
        )
        .route(
            "/api/processing-status",
            get(handlers::queue::get_processing_status),
        )
        .route(
            "/api/processing",
            post(handlers::queue::set_processing_status),
        )
        .route("/api/settings", get(handlers::admin::get_settings))
        .route("/api/settings", post(handlers::admin::update_settings))
        .route("/api/mcp/status", get(handlers::admin::get_mcp_status))
        .route("/api/mcp/toggle", post(handlers::admin::toggle_mcp))
        .route(
            "/api/branch/status",
            get(handlers::branch::get_branch_status),
        )
        .route("/api/branch/switch", post(handlers::branch::switch_branch))
        .route("/api/branch/update", post(handlers::branch::update_branch))
        .route("/api/instructions", get(handlers::admin::get_instructions))
        .route("/api/admin/restart", post(handlers::admin::admin_restart))
        .route("/api/admin/shutdown", post(handlers::admin::admin_shutdown))
        .route(
            "/api/admin/rebuild-embeddings",
            post(handlers::admin::rebuild_embeddings),
        )
        .route("/api/unified-search", get(handlers::search::unified_search))
        .route(
            "/api/unified-timeline",
            get(handlers::search::unified_timeline),
        )
        .route("/api/decisions", get(handlers::context::get_decisions))
        .route("/api/changes", get(handlers::context::get_changes))
        .route(
            "/api/how-it-works",
            get(handlers::context::get_how_it_works),
        )
        .route(
            "/api/context/timeline",
            get(handlers::context::context_timeline),
        )
        .route(
            "/api/context/preview",
            get(handlers::context::context_preview),
        )
        .route(
            "/api/timeline/by-query",
            get(handlers::context::context_timeline),
        )
        .route("/api/search/help", get(handlers::context::search_help))
        .route("/api/knowledge", get(handlers::knowledge::list_knowledge))
        .route(
            "/api/knowledge/search",
            get(handlers::knowledge::search_knowledge),
        )
        .route(
            "/api/knowledge/{id}",
            get(handlers::knowledge::get_knowledge_by_id)
                .delete(handlers::knowledge::delete_knowledge),
        )
        .route("/api/knowledge", post(handlers::knowledge::save_knowledge))
        .route(
            "/api/knowledge/{id}/usage",
            put(handlers::knowledge::record_knowledge_usage),
        )
        .route(
            "/api/knowledge/lifecycle",
            post(handlers::knowledge::run_confidence_lifecycle),
        )
        .route(
            "/api/infinite/expand_summary/{id}",
            get(handlers::infinite::infinite_expand_summary),
        )
        .route(
            "/api/infinite/time_range",
            get(handlers::infinite::infinite_time_range),
        )
        .route(
            "/api/infinite/drill_hour/{id}",
            get(handlers::infinite::infinite_drill_hour),
        )
        .route(
            "/api/infinite/drill_day/{id}",
            get(handlers::infinite::infinite_drill_day),
        )
        .route(
            "/api/infinite/search_entities",
            get(handlers::infinite::infinite_search_entities),
        )
        .with_state(state)
}

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let cb = state.search_service.circuit_breaker();
    let cb_state = cb.state_name();
    let is_open = cb.is_open();
    let seconds_until_probe = if is_open {
        Some(cb.seconds_until_probe())
    } else {
        None
    };
    let uptime_seconds = state.started_at.elapsed().as_secs();

    Json(HealthResponse {
        status: if is_open { "degraded" } else { "ok" },
        circuit_breaker: cb_state,
        seconds_until_probe,
        uptime_seconds,
    })
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
