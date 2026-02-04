//! Service layer for opencode-mem
//!
//! Centralizes business logic between HTTP/MCP handlers and storage/llm.

mod observation_service;
mod session_service;

pub use observation_service::ObservationService;
pub use session_service::SessionService;
