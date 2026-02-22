//! Storage backend trait abstraction
//!
//! Defines async domain traits for storage operations, enabling
//! PostgreSQL-based storage with tsvector + GIN for full-text search.

pub mod knowledge;
pub mod misc;
pub mod observation;
pub mod queue;
pub mod session;

pub use knowledge::KnowledgeStore;
pub use misc::{EmbeddingStore, InjectionStore, PromptStore, SearchStore, StatsStore};
pub use observation::ObservationStore;
pub use queue::PendingQueueStore;
pub use session::{SessionStore, SummaryStore};
