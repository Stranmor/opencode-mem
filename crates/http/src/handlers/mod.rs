#![allow(clippy::shadow_reuse, reason = "Shadowing for Arc clones is idiomatic")]
#![allow(clippy::shadow_unrelated, reason = "Shadowing in async blocks is idiomatic")]
#![allow(clippy::cognitive_complexity, reason = "Complex async handlers are inherent")]
#![allow(clippy::single_call_fn, reason = "HTTP handlers are called once from router")]

use crate::api_error::ApiError;

pub(crate) const fn is_localhost(addr: &std::net::SocketAddr) -> bool {
    addr.ip().is_loopback()
}

pub mod admin;
pub mod api_docs;
pub mod context;
pub mod infinite;
pub mod knowledge;
pub mod observations;
pub mod queue;
pub mod queue_processor;
pub mod search;
pub(crate) mod session_ops;
pub mod sessions;
pub mod sessions_api;
