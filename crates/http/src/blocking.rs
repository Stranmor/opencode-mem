//! Helpers for running blocking operations in async handlers.
//!
//! These helpers eliminate boilerplate for the common pattern of:
//! 1. Spawning a blocking task
//! 2. Handling join errors
//! 3. Handling storage/operation errors
//! 4. Wrapping result in Json

use axum::{http::StatusCode, Json};
use serde::Serialize;
use tokio::task::spawn_blocking;

/// Runs a blocking closure and returns `Result<Json<T>, StatusCode>`.
///
/// Use this for handlers that return JSON-wrapped results.
///
/// # Example
/// ```ignore
/// pub async fn get_projects(
///     State(state): State<Arc<AppState>>,
/// ) -> Result<Json<Vec<String>>, StatusCode> {
///     let storage = state.storage.clone();
///     blocking_json(move || storage.get_all_projects()).await
/// }
/// ```
pub async fn blocking_json<T, F>(f: F) -> Result<Json<T>, StatusCode>
where
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
    T: Send + 'static + Serialize,
{
    spawn_blocking(f)
        .await
        .map_err(|e| {
            tracing::error!("Join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(Json)
        .map_err(|e| {
            tracing::error!("Storage error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// Runs a blocking closure and returns `Result<T, StatusCode>` without Json wrapper.
///
/// Use this when you need the raw value for further processing.
///
/// # Example
/// ```ignore
/// let session = blocking_result(move || storage.get_session(&id)).await?;
/// // Now use session for further logic
/// ```
pub async fn blocking_result<T, F>(f: F) -> Result<T, StatusCode>
where
    F: FnOnce() -> anyhow::Result<T> + Send + 'static,
    T: Send + 'static,
{
    spawn_blocking(f)
        .await
        .map_err(|e| {
            tracing::error!("Join error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map_err(|e| {
            tracing::error!("Storage error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}
