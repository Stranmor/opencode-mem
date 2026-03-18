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

/// Admin access control with two modes:
/// - **Token configured** (`OPENCODE_MEM_ADMIN_TOKEN` set): require the token
///   for ALL requests regardless of source IP. Secure for production.
/// - **No token configured**: allow admin access from loopback IPs only
///   (127.0.0.1, ::1). Convenient for local development where the CLI
///   `serve` command tells users to `curl -X POST .../api/admin/shutdown`.
pub(crate) fn check_admin_access(
    addr: &std::net::SocketAddr,
    headers: &axum::http::HeaderMap,
    config: &opencode_mem_core::AppConfig,
) -> bool {
    if let Some(ref token) = config.admin_token {
        // Token mode: require valid token regardless of IP
        if let Some(provided) = headers.get("x-admin-token").and_then(|h| h.to_str().ok())
            && subtle::ConstantTimeEq::ct_eq(provided.as_bytes(), token.as_bytes()).into()
        {
            return true;
        }
        false
    } else {
        // No token mode: allow loopback only
        addr.ip().is_loopback()
    }
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
