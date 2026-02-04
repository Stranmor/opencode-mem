#![allow(
    clippy::multiple_inherent_impl,
    reason = "impl blocks split across files for organization"
)]
#![allow(unreachable_pub, reason = "pub items in pub(crate) modules are intentional")]

pub(crate) mod ai_types;
pub(crate) mod client;
pub(crate) mod knowledge;
pub(crate) mod observation;
pub(crate) mod summary;

pub use client::LlmClient;

#[cfg(test)]
mod tests;
