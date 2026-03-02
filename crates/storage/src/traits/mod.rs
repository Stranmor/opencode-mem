//! Storage backend trait abstraction
//!
//! Defines async domain traits for storage operations, enabling
//! PostgreSQL-based storage with tsvector + GIN for full-text search.

pub mod embedding;
pub mod injection;
pub mod knowledge;
pub mod observation;
pub mod prompt;
pub mod queue;
pub mod search;
pub mod session;
pub mod stats;

pub use embedding::EmbeddingStore;
pub use injection::InjectionStore;
pub use knowledge::KnowledgeStore;
pub use observation::ObservationStore;
pub use prompt::PromptStore;
pub use queue::PendingQueueStore;
pub use search::SearchStore;
pub use session::{SessionStore, SummaryStore};
pub use stats::StatsStore;
