//! Service layer for opencode-mem
//!
//! Centralizes business logic between HTTP/MCP handlers and storage/llm.

#![allow(missing_docs, reason = "Internal crate with self-explanatory API")]
#![allow(clippy::missing_errors_doc, reason = "Errors are self-explanatory from Result types")]
#![allow(clippy::ref_patterns, reason = "Ref patterns are clearer in some contexts")]
#![allow(missing_debug_implementations, reason = "Internal types")]
#![allow(clippy::manual_let_else, reason = "if let is clearer")]
#![allow(clippy::let_underscore_untyped, reason = "Type is clear from context")]
#![allow(clippy::let_underscore_must_use, reason = "Intentionally ignoring results")]
#![allow(let_underscore_drop, reason = "Intentionally dropping values")]
#![allow(clippy::missing_docs_in_private_items, reason = "Internal crate")]
#![allow(clippy::implicit_return, reason = "Implicit return is idiomatic Rust")]
#![allow(clippy::question_mark_used, reason = "? operator is idiomatic Rust")]
#![allow(clippy::cognitive_complexity, reason = "Complex async flows are inherent")]
#![allow(clippy::min_ident_chars, reason = "Short error vars are idiomatic")]

mod knowledge_service;
mod observation_service;
mod queue_service;
mod search_service;
mod session_service;

pub use knowledge_service::KnowledgeService;
pub use observation_service::ObservationService;
pub use queue_service::QueueService;
pub use search_service::SearchService;
pub use session_service::SessionService;
