//! LLM client for observation compression and knowledge extraction.

#![allow(
    clippy::multiple_inherent_impl,
    reason = "impl blocks split across files for organization"
)]
#![allow(unreachable_pub, reason = "pub items in pub(crate) modules are intentional")]
#![allow(clippy::missing_docs_in_private_items, reason = "Internal crate modules")]
#![allow(clippy::implicit_return, reason = "Implicit return is idiomatic Rust")]
#![allow(clippy::question_mark_used, reason = "? operator is idiomatic Rust")]
#![allow(clippy::min_ident_chars, reason = "Short closure params are idiomatic")]
#![allow(clippy::single_call_fn, reason = "Helper functions improve readability")]

mod ai_types;
mod client;
pub mod error;
mod insights;
mod knowledge;
mod observation;
mod summary;

pub use client::LlmClient;
pub use error::LlmError;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod retry_tests;
