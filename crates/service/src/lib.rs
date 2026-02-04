//! Service layer for opencode-mem
//!
//! Centralizes business logic between HTTP/MCP handlers and storage/llm.

#![allow(clippy::missing_errors_doc, reason = "Errors are self-explanatory from Result types")]
#![allow(clippy::ref_patterns, reason = "Ref patterns are clearer in some contexts")]
#![allow(missing_debug_implementations, reason = "Internal types")]
#![allow(clippy::manual_let_else, reason = "if let is clearer")]
#![allow(clippy::let_underscore_untyped, reason = "Type is clear from context")]
#![allow(clippy::let_underscore_must_use, reason = "Intentionally ignoring results")]
#![allow(let_underscore_drop, reason = "Intentionally dropping values")]

mod observation_service;
mod session_service;

pub use observation_service::ObservationService;
pub use session_service::SessionService;
