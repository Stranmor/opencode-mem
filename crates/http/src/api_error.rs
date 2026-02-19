//! Typed API error for HTTP handlers.
//!
//! Converts domain errors into proper HTTP responses with JSON body and status codes.
//! Handlers can return `Result<Json<T>, ApiError>` instead of losing error context
//! with bare `StatusCode`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use opencode_mem_storage::StorageError;

/// API error with HTTP status code and human-readable message.
///
/// Use via `Result<Json<T>, ApiError>` in handlers.
/// Converts to JSON response: `{"error": "message"}`.
///
/// `Internal` variant logs the real error server-side and returns
/// a static message to the client — no error detail leakage.
#[derive(Debug)]
pub enum ApiError {
    /// 400 Bad Request — invalid input from caller.
    BadRequest(String),
    /// 403 Forbidden — action not allowed (e.g., non-localhost admin request).
    Forbidden(String),
    /// 404 Not Found — requested resource doesn't exist.
    NotFound(String),
    /// 422 Unprocessable Entity — valid syntax but semantic rejection (e.g., duplicate).
    UnprocessableEntity(String),
    /// 500 Internal Server Error — unexpected failure. Details logged, not exposed.
    Internal(anyhow::Error),
    /// 503 Service Unavailable — required backend not configured.
    ServiceUnavailable(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            Self::UnprocessableEntity(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg),
            Self::Internal(err) => {
                tracing::error!(error = ?err, "internal server error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error".to_owned())
            },
            Self::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
        };
        let body = serde_json::json!({"error": message});
        (status, Json(body)).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
}

impl From<opencode_mem_service::ServiceError> for ApiError {
    fn from(err: opencode_mem_service::ServiceError) -> Self {
        use opencode_mem_service::ServiceError;
        match err {
            ServiceError::Storage(ref e) if e.is_duplicate() => {
                Self::UnprocessableEntity(err.to_string())
            },
            ServiceError::Storage(StorageError::NotFound { entity, id }) => {
                Self::NotFound(format!("{entity} '{id}' not found"))
            },
            ServiceError::InvalidInput(msg) => Self::BadRequest(msg),
            ServiceError::NotConfigured(msg) => Self::ServiceUnavailable(msg),
            _ => Self::Internal(err.into()),
        }
    }
}
