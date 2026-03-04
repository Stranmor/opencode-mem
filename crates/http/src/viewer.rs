//! Web Viewer UI - embedded HTML/CSS/JS for memory observation viewer
//!
//! Serves a dark-themed single-page app at `/` with:
//! - Real-time observation stream via SSE
//! - Search functionality
//! - Timeline view with observation cards

use axum::{
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
};

/// Embedded HTML for the viewer UI
pub const VIEWER_HTML: &str = include_str!("viewer.html");

/// Serve the viewer HTML page
pub async fn serve_viewer() -> Response {
    (StatusCode::OK, [(header::CONTENT_TYPE, "text/html; charset=utf-8")], Html(VIEWER_HTML))
        .into_response()
}
