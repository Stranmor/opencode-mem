//! Circuit breaker for PostgreSQL connection health tracking.
//!
//! Prevents flooding the database with reconnection attempts when the server
//! is down. Uses exponential backoff with three states:
//!
//! - **Closed**: Normal operation. All queries go through.
//! - **Open**: Database is unavailable. Queries fast-fail with `StorageError::Unavailable`.
//! - **HalfOpen**: Probe period. One query is allowed through to test connectivity.
//!
//! The circuit breaker is shared across all storage operations via `Arc`.

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Circuit breaker states stored as u8 for atomic operations.
const STATE_CLOSED: u8 = 0;
const STATE_OPEN: u8 = 1;
const STATE_HALF_OPEN: u8 = 2;

/// Minimum backoff duration when the circuit opens (30 seconds).
const MIN_BACKOFF_SECS: u64 = 30;

/// Maximum backoff duration (5 minutes).
const MAX_BACKOFF_SECS: u64 = 300;

/// Number of consecutive failures before the circuit opens.
const FAILURE_THRESHOLD: u64 = 3;

/// Circuit breaker for database connection health tracking.
///
/// Thread-safe (lock-free atomics). Wrap in `Arc` and share across `PgStorage`.
#[derive(Debug)]
pub struct CircuitBreaker {
    /// Current state: 0=Closed, 1=Open, 2=HalfOpen
    state: AtomicU8,
    /// Count of consecutive failures
    failure_count: AtomicU64,
    /// Monotonic timestamp when the circuit last opened
    last_failure_time: std::sync::RwLock<Instant>,
    /// Current backoff duration in seconds (doubles on each re-open)
    backoff_secs: AtomicU64,
}

impl CircuitBreaker {
    /// Create a new circuit breaker in the closed (healthy) state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: AtomicU8::new(STATE_CLOSED),
            failure_count: AtomicU64::new(0),
            last_failure_time: std::sync::RwLock::new(Instant::now()),
            backoff_secs: AtomicU64::new(MIN_BACKOFF_SECS),
        }
    }

    /// Check whether a request should be allowed through.
    ///
    /// Returns `true` if the request can proceed (circuit closed or half-open probe).
    /// Returns `false` if the circuit is open and backoff has not elapsed.
    pub fn should_allow(&self) -> bool {
        let state = self.state.load(Ordering::Acquire);
        match state {
            STATE_CLOSED => true,
            STATE_OPEN => {
                let now = Instant::now();
                let last_failure = *self
                    .last_failure_time
                    .read()
                    .unwrap_or_else(|e| e.into_inner());
                let backoff = Duration::from_secs(self.backoff_secs.load(Ordering::Relaxed));
                if now.duration_since(last_failure) >= backoff {
                    // Transition to half-open: allow one probe request.
                    // Use compare_exchange to avoid thundering herd.
                    if self
                        .state
                        .compare_exchange(
                            STATE_OPEN,
                            STATE_HALF_OPEN,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        tracing::info!(
                            elapsed_secs = now.duration_since(last_failure).as_secs(),
                            backoff_secs = backoff.as_secs(),
                            "Circuit breaker: Open → HalfOpen (probing database)"
                        );
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            STATE_HALF_OPEN => {
                // Only one probe at a time — additional requests during half-open fast-fail
                // to avoid hammering a recovering database.
                false
            }
            _ => true, // Unknown state — allow through
        }
    }

    /// Record a successful database operation.
    ///
    /// Resets the circuit to closed state, clears failure count and backoff.
    /// Returns `true` if this success caused a recovery transition (Open/HalfOpen → Closed),
    /// meaning this is the first success after a failure period.
    pub fn record_success(&self) -> bool {
        // Zero failure count FIRST to avoid race condition with record_failure
        self.failure_count.store(0, Ordering::Release);
        self.backoff_secs.store(MIN_BACKOFF_SECS, Ordering::Relaxed);
        let prev = self.state.swap(STATE_CLOSED, Ordering::Release);
        let recovered = prev != STATE_CLOSED;
        if recovered {
            tracing::info!("Circuit breaker: → Closed (database recovered)");
        }
        recovered
    }

    /// Record a failed database operation.
    ///
    /// Increments failure count. If threshold is reached, opens the circuit
    /// with exponential backoff.
    pub fn record_failure(&self) {
        let state = self.state.load(Ordering::Acquire);

        if state == STATE_HALF_OPEN {
            // Probe failed — re-open with doubled backoff
            let current_backoff = self.backoff_secs.load(Ordering::Relaxed);
            let new_backoff = current_backoff.saturating_mul(2).min(MAX_BACKOFF_SECS);
            self.backoff_secs.store(new_backoff, Ordering::Relaxed);
            if let Ok(mut lock) = self.last_failure_time.write() {
                *lock = Instant::now();
            }
            self.state.store(STATE_OPEN, Ordering::Release);
            tracing::warn!(
                backoff_secs = new_backoff,
                "Circuit breaker: HalfOpen → Open (probe failed, increasing backoff)"
            );
        } else {
            let count = self
                .failure_count
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1);
            if count >= FAILURE_THRESHOLD {
                if let Ok(mut lock) = self.last_failure_time.write() {
                    *lock = Instant::now();
                }
                let prev = self.state.swap(STATE_OPEN, Ordering::Release);
                if prev != STATE_OPEN {
                    let backoff = self.backoff_secs.load(Ordering::Relaxed);
                    tracing::warn!(
                        consecutive_failures = count,
                        backoff_secs = backoff,
                        "Circuit breaker: → Open (database unavailable)"
                    );
                }
            }
        }
    }

    /// Whether the circuit is currently open (database considered unavailable).
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.state.load(Ordering::Acquire) == STATE_OPEN
    }

    /// Whether the circuit is in half-open (probing) state.
    #[must_use]
    pub fn is_half_open(&self) -> bool {
        self.state.load(Ordering::Acquire) == STATE_HALF_OPEN
    }

    /// Current state as a human-readable string (for diagnostics).
    #[must_use]
    pub fn state_name(&self) -> &'static str {
        match self.state.load(Ordering::Acquire) {
            STATE_CLOSED => "closed",
            STATE_OPEN => "open",
            STATE_HALF_OPEN => "half-open",
            _ => "unknown",
        }
    }

    /// Seconds until the next probe attempt (0 if circuit is closed or probe is due).
    #[must_use]
    pub fn seconds_until_probe(&self) -> u64 {
        if self.state.load(Ordering::Acquire) != STATE_OPEN {
            return 0;
        }
        let now = Instant::now();
        let last_failure = *self
            .last_failure_time
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let backoff = Duration::from_secs(self.backoff_secs.load(Ordering::Relaxed));
        let target = last_failure + backoff;
        if now >= target {
            0
        } else {
            (target - now).as_secs()
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_circuit_breaker_is_closed() {
        let cb = CircuitBreaker::new();
        assert!(cb.should_allow());
        assert!(!cb.is_open());
        assert_eq!(cb.state_name(), "closed");
    }

    #[test]
    fn test_opens_after_threshold() {
        let cb = CircuitBreaker::new();
        for _ in 0..FAILURE_THRESHOLD {
            cb.record_failure();
        }
        assert!(cb.is_open());
        assert_eq!(cb.state_name(), "open");
        // Should fast-fail immediately (backoff hasn't elapsed)
        assert!(!cb.should_allow());
    }

    #[test]
    fn test_stays_closed_below_threshold() {
        let cb = CircuitBreaker::new();
        for _ in 0..(FAILURE_THRESHOLD.saturating_sub(1)) {
            cb.record_failure();
        }
        assert!(!cb.is_open());
        assert!(cb.should_allow());
    }

    #[test]
    fn test_success_resets() {
        let cb = CircuitBreaker::new();
        for _ in 0..FAILURE_THRESHOLD {
            cb.record_failure();
        }
        assert!(cb.is_open());
        cb.record_success();
        assert!(!cb.is_open());
        assert!(cb.should_allow());
        assert_eq!(cb.state_name(), "closed");
    }

    #[test]
    fn test_half_open_probe_failure_doubles_backoff() {
        let cb = CircuitBreaker::new();
        // Open the circuit
        for _ in 0..FAILURE_THRESHOLD {
            cb.record_failure();
        }
        // Force into half-open
        cb.state.store(STATE_HALF_OPEN, Ordering::Release);
        // Record failure during half-open
        cb.record_failure();
        assert!(cb.is_open());
        let backoff = cb.backoff_secs.load(Ordering::Relaxed);
        assert_eq!(backoff, MIN_BACKOFF_SECS.saturating_mul(2));
    }

    #[test]
    fn test_backoff_capped_at_max() {
        let cb = CircuitBreaker::new();
        cb.backoff_secs.store(MAX_BACKOFF_SECS, Ordering::Relaxed);
        cb.state.store(STATE_HALF_OPEN, Ordering::Release);
        cb.record_failure();
        let backoff = cb.backoff_secs.load(Ordering::Relaxed);
        assert_eq!(backoff, MAX_BACKOFF_SECS);
    }

    #[test]
    fn test_record_success_returns_recovery_signal() {
        let cb = CircuitBreaker::new();
        // Normal closed state — no recovery
        assert!(!cb.record_success());
        assert!(!cb.record_success());

        // Open the circuit
        for _ in 0..FAILURE_THRESHOLD {
            cb.record_failure();
        }
        assert!(cb.is_open());

        // Recovery from open → closed
        assert!(cb.record_success());
        // Subsequent successes are not recovery
        assert!(!cb.record_success());
    }

    #[test]
    fn test_record_success_returns_recovery_from_half_open() {
        let cb = CircuitBreaker::new();
        // Force into half-open
        cb.state.store(STATE_HALF_OPEN, Ordering::Release);

        // Recovery from half-open → closed
        assert!(cb.record_success());
        assert!(!cb.record_success());
    }
}
