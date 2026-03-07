//! In-memory buffer for write operations during degraded mode.
//!
//! When the circuit breaker is open (DB unavailable), write operations are
//! buffered here instead of being silently dropped. On recovery, the queue
//! is flushed back to the database.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Maximum items in the pending write queue to prevent OOM.
const MAX_QUEUE_SIZE: usize = 1000;

pub enum PendingWrite {
    SaveMemory {
        id: String,
        text: String,
        title: Option<String>,
        project: Option<String>,
        observation_type: Option<opencode_mem_core::ObservationType>,
        noise_level: Option<opencode_mem_core::NoiseLevel>,
    },
    DeleteObservation {
        id: String,
    },
    SaveKnowledge {
        id: String,
        input: opencode_mem_core::KnowledgeInput,
    },
    DeleteKnowledge {
        id: String,
    },
}

/// In-memory buffer for write operations when the database is unavailable.
///
/// Thread-safe (interior Mutex). Best-effort, at-most-once delivery:
/// if the process crashes, buffered writes are lost.
pub struct PendingWriteQueue {
    queue: Mutex<VecDeque<PendingWrite>>,
    flushing: AtomicBool,
}

impl PendingWriteQueue {
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            flushing: AtomicBool::new(false),
        }
    }

    /// Returns `false` if the queue was full (oldest item was dropped to make room).
    pub fn push(&self, item: PendingWrite) -> bool {
        let Ok(mut queue) = self.queue.lock() else {
            tracing::warn!("PendingWriteQueue mutex poisoned, dropping write");
            return false;
        };
        let mut dropped = false;
        while queue.len() >= MAX_QUEUE_SIZE {
            queue.pop_front();
            dropped = true;
        }
        if dropped {
            tracing::warn!(
                max = MAX_QUEUE_SIZE,
                "Pending write queue full, dropped oldest item(s)"
            );
        }
        queue.push_back(item);
        !dropped
    }

    pub fn drain_all(&self) -> Vec<PendingWrite> {
        let Ok(mut queue) = self.queue.lock() else {
            tracing::warn!("PendingWriteQueue mutex poisoned during drain");
            return Vec::new();
        };
        queue.drain(..).collect()
    }

    pub fn pop_front(&self) -> Option<PendingWrite> {
        let Ok(mut queue) = self.queue.lock() else {
            tracing::warn!("PendingWriteQueue mutex poisoned during pop_front");
            return None;
        };
        queue.pop_front()
    }

    pub fn push_front(&self, item: PendingWrite) {
        let Ok(mut queue) = self.queue.lock() else {
            tracing::warn!("PendingWriteQueue mutex poisoned during push_front");
            return;
        };
        if queue.len() >= MAX_QUEUE_SIZE {
            queue.pop_back();
            tracing::warn!(
                max = MAX_QUEUE_SIZE,
                "Pending write queue full on push_front, dropped newest item"
            );
        }
        queue.push_front(item);
    }

    pub fn start_flush(self: &Arc<Self>) -> Option<FlushGuard> {
        if self
            .flushing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            Some(FlushGuard {
                queue: Arc::clone(self),
            })
        } else {
            None
        }
    }

    pub fn finish_flush(&self) {
        self.flushing.store(false, Ordering::Release);
    }

    pub fn len(&self) -> usize {
        self.queue.lock().map(|q| q.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for PendingWriteQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard to ensure `flushing` flag is always reset even on panic.
pub struct FlushGuard {
    queue: Arc<PendingWriteQueue>,
}

impl Drop for FlushGuard {
    fn drop(&mut self) {
        self.queue.finish_flush();
    }
}

pub fn spawn_pending_flush(
    observation_service: &Arc<crate::ObservationService>,
    knowledge_service: &Arc<crate::KnowledgeService>,
    pending_writes_ref: &Arc<PendingWriteQueue>,
) {
    if pending_writes_ref.is_empty() {
        return;
    }
    let guard = match pending_writes_ref.start_flush() {
        Some(g) => g,
        None => return,
    };

    let observation_service = Arc::clone(observation_service);
    let knowledge_service = Arc::clone(knowledge_service);
    let pending_writes = Arc::clone(pending_writes_ref);
    tokio::spawn(async move {
        // Transfer guard to task
        let _guard = guard;
        let pending_count = pending_writes.len();
        tracing::info!(pending_count, "Flushing pending writes after DB recovery");

        loop {
            let Some(item) = pending_writes.pop_front() else {
                break;
            };

            match item {
                PendingWrite::SaveMemory {
                    id,
                    text,
                    title,
                    project,
                    observation_type,
                    noise_level,
                } => {
                    match observation_service
                        .save_memory_with_id(
                            &id,
                            &text,
                            title.as_deref(),
                            project.as_deref(),
                            observation_type,
                            noise_level,
                        )
                        .await
                    {
                        Ok(_) => {}
                        Err(e) if e.is_db_unavailable() || e.is_transient() => {
                            pending_writes.push_front(PendingWrite::SaveMemory {
                                id,
                                text,
                                title,
                                project,
                                observation_type,
                                noise_level,
                            });
                            tracing::warn!(
                                error = %e,
                                remaining = pending_writes.len(),
                                "Pending write flush paused: database became unavailable again"
                            );
                            break;
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Pending save_memory flush dropped one invalid item");
                        }
                    }
                }
                PendingWrite::DeleteObservation { id } => {
                    match observation_service.delete_observation(&id).await {
                        Ok(_) => {}
                        Err(e) if e.is_db_unavailable() || e.is_transient() => {
                            pending_writes.push_front(PendingWrite::DeleteObservation { id });
                            break;
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Pending delete_observation flush failed");
                        }
                    }
                }
                PendingWrite::SaveKnowledge { id, input } => {
                    match knowledge_service
                        .save_knowledge_with_id(&id, input.clone())
                        .await
                    {
                        Ok(_) => {}
                        Err(e) if e.is_db_unavailable() || e.is_transient() => {
                            pending_writes.push_front(PendingWrite::SaveKnowledge { id, input });
                            break;
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Pending save_knowledge flush failed");
                        }
                    }
                }
                PendingWrite::DeleteKnowledge { id } => {
                    match knowledge_service.delete_knowledge(&id).await {
                        Ok(_) => {}
                        Err(e) if e.is_db_unavailable() || e.is_transient() => {
                            pending_writes.push_front(PendingWrite::DeleteKnowledge { id });
                            break;
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Pending delete_knowledge flush failed");
                        }
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_drain() {
        let q = PendingWriteQueue::new();
        assert!(q.is_empty());

        q.push(PendingWrite::SaveMemory {
            id: "test-id".to_owned(),
            text: "hello".into(),
            title: None,
            project: None,
            observation_type: None,
            noise_level: None,
        });
        assert_eq!(q.len(), 1);

        let items = q.drain_all();
        assert_eq!(items.len(), 1);
        assert!(q.is_empty());
    }

    #[test]
    fn test_overflow_drops_oldest() {
        let q = PendingWriteQueue::new();
        for i in 0..MAX_QUEUE_SIZE {
            q.push(PendingWrite::SaveMemory {
                id: format!("id-{i}"),
                text: format!("item-{i}"),
                title: None,
                project: None,
                observation_type: None,
                noise_level: None,
            });
        }
        assert_eq!(q.len(), MAX_QUEUE_SIZE);

        let accepted = q.push(PendingWrite::SaveMemory {
            id: "overflow-id".to_owned(),
            text: "overflow".into(),
            title: None,
            project: None,
            observation_type: None,
            noise_level: None,
        });
        assert!(!accepted);
        assert_eq!(q.len(), MAX_QUEUE_SIZE);

        let items = q.drain_all();
        assert_eq!(items.len(), MAX_QUEUE_SIZE);
        // First item should be item-1 (item-0 was dropped)
        match items.first() {
            Some(PendingWrite::SaveMemory { text, .. }) => assert_eq!(text, "item-1"),
            _ => panic!("Expected SaveMemory"),
        }
    }
}
