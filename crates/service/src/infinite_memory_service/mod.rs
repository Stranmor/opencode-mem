mod compression;
mod pipeline;
mod queries;

use opencode_mem_core::{InfiniteSummary, RawInfiniteEvent, StoredInfiniteEvent, SummaryEntities};
use opencode_mem_llm::LlmClient;
use opencode_mem_storage::{CircuitBreaker, StorageError};

use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub use compression::init_compression_config;

/// RAII guard that resets `migrations_pending` to `true` on drop (failure).
/// On successful migration, call `guard.mark_success()` — Drop runs but
/// doesn't reset the flag. No `std::mem::forget` needed (avoids Arc leak).
struct MigrationGuard {
    flag: Arc<AtomicBool>,
    succeeded: bool,
}

impl MigrationGuard {
    fn new(flag: Arc<AtomicBool>) -> Self {
        Self {
            flag,
            succeeded: false,
        }
    }

    fn mark_success(&mut self) {
        self.succeeded = true;
    }
}

impl Drop for MigrationGuard {
    fn drop(&mut self) {
        if !self.succeeded {
            self.flag.store(true, Ordering::Release);
        }
    }
}

#[derive(Clone)]
pub struct InfiniteMemoryService {
    pool: PgPool,
    llm: Arc<LlmClient>,
    circuit_breaker: Arc<CircuitBreaker>,
    migrations_pending: Arc<AtomicBool>,
}

impl InfiniteMemoryService {
    pub async fn new(pool: sqlx::PgPool, llm: Arc<LlmClient>) -> Result<Self> {
        let migrations_pending =
            match opencode_mem_storage::pg_storage::infinite_memory::run_infinite_memory_migrations(
                &pool,
            )
            .await
            {
                Ok(()) => {
                    tracing::info!("Infinite Memory migrations completed");
                    false
                }
                Err(e) => {
                    tracing::warn!(
                        "Infinite Memory started without migrations (DB may be unavailable): {e}"
                    );
                    true
                }
            };

        let svc = Self {
            pool,
            llm,
            circuit_breaker: Arc::new(CircuitBreaker::new()),
            migrations_pending: Arc::new(AtomicBool::new(migrations_pending)),
        };

        // Spawn a background loop to retry deferred migrations periodically.
        // Solves the deadlock where migrations are deferred (DB was down at startup)
        // but DB operations fail with "relation does not exist" (non-transient),
        // so record_result never triggers retry.
        if migrations_pending {
            let svc_clone = svc.clone();
            tokio::spawn(async move {
                const INITIAL_DELAY_SECS: u64 = 30;
                const MAX_DELAY_SECS: u64 = 600; // 10 minutes
                let mut delay_secs = INITIAL_DELAY_SECS;
                let mut attempt: u32 = 0;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                    if !svc_clone.has_pending_migrations() {
                        break;
                    }
                    attempt = attempt.saturating_add(1);
                    if svc_clone.try_run_migrations().await {
                        tracing::info!(
                            "Infinite Memory deferred migrations resolved by background retry"
                        );
                        break;
                    }
                    let next_delay = delay_secs.saturating_mul(2).min(MAX_DELAY_SECS);
                    tracing::warn!(
                        attempt,
                        next_retry_secs = next_delay,
                        "Infinite Memory migration retry failed, backing off"
                    );
                    delay_secs = next_delay;
                }
            });
        }

        Ok(svc)
    }

    #[must_use]
    pub fn new_degraded(pool: sqlx::PgPool, llm: Arc<LlmClient>) -> Self {
        let cb = CircuitBreaker::new();
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        Self {
            pool,
            llm,
            circuit_breaker: Arc::new(cb),
            migrations_pending: Arc::new(AtomicBool::new(true)),
        }
    }

    #[must_use]
    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }

    pub async fn try_run_migrations(&self) -> bool {
        // CAS as the actual lock: only one caller proceeds
        if self
            .migrations_pending
            .compare_exchange(true, false, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            return false;
        }

        let mut guard = MigrationGuard::new(Arc::clone(&self.migrations_pending));
        match opencode_mem_storage::pg_storage::infinite_memory::run_infinite_memory_migrations(
            &self.pool,
        )
        .await
        {
            Ok(()) => {
                guard.mark_success();
                tracing::info!("Infinite Memory deferred migrations completed successfully");
                true
            }
            Err(e) => {
                drop(guard);
                tracing::warn!("Infinite Memory deferred migration attempt failed: {e}");
                false
            }
        }
    }

    #[must_use]
    pub fn has_pending_migrations(&self) -> bool {
        self.migrations_pending.load(Ordering::Acquire)
    }

    pub async fn store_event(&self, event: RawInfiniteEvent) -> Result<i64, StorageError> {
        self.guarded(|| {
            opencode_mem_storage::pg_storage::infinite_memory::store_infinite_event(
                &self.pool,
                event.clone(),
            )
        })
        .await
    }

    pub async fn compress_events(
        &self,
        events: &[StoredInfiniteEvent],
    ) -> Result<(String, Option<SummaryEntities>)> {
        compression::compress_events(&self.llm, events).await
    }

    pub async fn create_5min_summary(
        &self,
        events: &[StoredInfiniteEvent],
        summary: &str,
        entities: Option<&SummaryEntities>,
    ) -> Result<i64, StorageError> {
        let events = events.to_vec();
        let summary = summary.to_owned();
        let entities = entities.cloned();
        self.guarded(|| {
            opencode_mem_storage::pg_storage::infinite_memory::create_5min_summary(
                &self.pool,
                &events,
                &summary,
                entities.as_ref(),
            )
        })
        .await
    }

    pub async fn run_compression_pipeline(&self) -> Result<u32> {
        if !self.circuit_breaker.should_allow() {
            return Err(anyhow::anyhow!(
                "circuit breaker open, {} seconds until probe",
                self.circuit_breaker.seconds_until_probe()
            ));
        }
        let result = pipeline::run_compression_pipeline(&self.pool, &self.llm).await;
        match &result {
            Ok(_) => {
                self.circuit_breaker.record_success();
            }
            Err(_) => {
                self.circuit_breaker.record_failure();
            }
        }
        result
    }

    pub async fn create_hour_summary(
        &self,
        summaries: &[InfiniteSummary],
        content: &str,
        entities: Option<&SummaryEntities>,
    ) -> Result<i64, StorageError> {
        let summaries = summaries.to_vec();
        let content = content.to_owned();
        let entities = entities.cloned();
        self.guarded(|| {
            opencode_mem_storage::pg_storage::infinite_memory::create_hour_summary(
                &self.pool,
                &summaries,
                &content,
                entities.as_ref(),
            )
        })
        .await
    }

    pub async fn create_day_summary(
        &self,
        summaries: &[InfiniteSummary],
        content: &str,
        entities: Option<&SummaryEntities>,
    ) -> Result<i64, StorageError> {
        let summaries = summaries.to_vec();
        let content = content.to_owned();
        let entities = entities.cloned();
        self.guarded(|| {
            opencode_mem_storage::pg_storage::infinite_memory::create_day_summary(
                &self.pool,
                &summaries,
                &content,
                entities.as_ref(),
            )
        })
        .await
    }

    pub async fn compress_summaries(&self, summaries: &[InfiniteSummary]) -> Result<String> {
        compression::compress_summaries(&self.llm, summaries).await
    }

    pub async fn run_full_compression(&self) -> Result<(u32, u32, u32)> {
        if !self.circuit_breaker.should_allow() {
            return Err(anyhow::anyhow!(
                "circuit breaker open, {} seconds until probe",
                self.circuit_breaker.seconds_until_probe()
            ));
        }
        let result = pipeline::run_full_compression(&self.pool, &self.llm).await;
        match &result {
            Ok(_) => {
                self.circuit_breaker.record_success();
            }
            Err(_) => {
                self.circuit_breaker.record_failure();
            }
        }
        result
    }

    pub async fn guarded<F, Fut, T>(&self, op_f: F) -> Result<T, StorageError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, StorageError>>,
    {
        if !self.circuit_breaker.should_allow() {
            return Err(StorageError::Unavailable {
                seconds_until_probe: self.circuit_breaker.seconds_until_probe(),
            });
        }
        let result = op_f().await;
        self.record_result_storage(&result);
        result
    }

    fn record_result_storage<T>(&self, result: &Result<T, StorageError>) {
        if result.is_ok() {
            let recovered = self.circuit_breaker.record_success();
            if recovered && self.migrations_pending.load(Ordering::Acquire) {
                let this = self.clone();
                // Acquire lock synchronously BEFORE spawning to avoid thundering herd
                if this
                    .migrations_pending
                    .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    tokio::spawn(async move {
                        let mut guard = MigrationGuard::new(Arc::clone(&this.migrations_pending));
                        match opencode_mem_storage::pg_storage::infinite_memory::run_infinite_memory_migrations(
                            &this.pool,
                        )
                        .await
                        {
                            Ok(()) => {
                                guard.mark_success();
                                tracing::info!(
                                    "Infinite Memory deferred migrations completed successfully"
                                );
                            }
                            Err(e) => {
                                drop(guard);
                                tracing::warn!(
                                    "Infinite Memory deferred migration attempt failed: {e}"
                                );
                            }
                        }
                    });
                }
            }
        } else if let Err(e) = result {
            if e.is_transient() {
                self.circuit_breaker.record_failure();
            } else if self.circuit_breaker.is_half_open() {
                self.circuit_breaker.record_failure();
                tracing::debug!(
                    "Non-transient error during HalfOpen probe, recording failure: {}",
                    e
                );
            } else {
                tracing::debug!("Non-transient error, not tripping circuit breaker: {}", e);
            }

            if e.is_missing_relation() && self.migrations_pending.load(Ordering::Acquire) {
                let this = self.clone();
                // Acquire lock synchronously BEFORE spawning to avoid thundering herd
                if this
                    .migrations_pending
                    .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    tokio::spawn(async move {
                        let mut guard = MigrationGuard::new(Arc::clone(&this.migrations_pending));
                        match opencode_mem_storage::pg_storage::infinite_memory::run_infinite_memory_migrations(
                            &this.pool,
                        )
                        .await
                        {
                            Ok(()) => {
                                guard.mark_success();
                                tracing::info!(
                                    "Infinite Memory deferred migrations completed successfully"
                                );
                            }
                            Err(e) => {
                                drop(guard);
                                tracing::warn!(
                                    "Infinite Memory deferred migration attempt failed: {e}"
                                );
                            }
                        }
                    });
                }
            }
        }
    }
}
