#![allow(clippy::shadow_reuse, reason = "Shadowing for Arc clones is idiomatic")]
#![allow(
    clippy::shadow_unrelated,
    reason = "Shadowing in async blocks is idiomatic"
)]
#![allow(
    clippy::cognitive_complexity,
    reason = "Complex async handlers are inherent"
)]
#![allow(
    clippy::single_call_fn,
    reason = "HTTP handlers are called once from router"
)]

pub(crate) fn check_admin_access(
    addr: &std::net::SocketAddr,
    headers: &axum::http::HeaderMap,
    config: &opencode_mem_core::AppConfig,
) -> bool {
    // 1. Check for explicit admin token (required for non-localhost or behind proxy)
    if let Some(ref token) = config.admin_token
        && let Some(provided) = headers.get("x-admin-token").and_then(|h| h.to_str().ok())
        && provided == token
    {
        return true;
    }

    // 2. Fallback to loopback check (only if no token is configured)
    if config.admin_token.is_none() && addr.ip().is_loopback() {
        return true;
    }

    false
}

pub mod admin;
pub mod api_docs;
pub mod branch;
pub mod context;
pub(crate) mod cron;
pub mod infinite;
pub mod knowledge;
pub mod observations;
pub mod queue;
pub mod queue_processor;
pub mod search;
pub(crate) mod session_ops;
pub mod sessions;
pub mod sessions_api;

#[cfg(test)]
mod breaker_tests;
