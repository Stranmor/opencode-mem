//! In-memory buffer for write operations during degraded mode.
//!
//! When the circuit breaker is open (DB unavailable), write operations are
//! buffered here instead of being silently dropped. On recovery, the queue
//! is flushed back to the database.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Maximum items in the pending write queue to prevent OOM.
const MAX_QUEUE_SIZE: usize = 1000;

pub enum PendingWrite {
    SaveMemory {
        text: String,
        title: Option<String>,
        project: Option<String>,
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
        if queue.len() >= MAX_QUEUE_SIZE {
            queue.pop_front();
            tracing::warn!(
                max = MAX_QUEUE_SIZE,
                "Pending write queue full, dropping oldest item"
            );
            queue.push_back(item);
            false
        } else {
            queue.push_back(item);
            true
        }
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
        queue.push_front(item);
    }

    pub fn start_flush(&self) -> bool {
        self.flushing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_drain() {
        let q = PendingWriteQueue::new();
        assert!(q.is_empty());

        q.push(PendingWrite::SaveMemory {
            text: "hello".into(),
            title: None,
            project: None,
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
                text: format!("item-{i}"),
                title: None,
                project: None,
            });
        }
        assert_eq!(q.len(), MAX_QUEUE_SIZE);

        let accepted = q.push(PendingWrite::SaveMemory {
            text: "overflow".into(),
            title: None,
            project: None,
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
