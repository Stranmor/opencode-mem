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
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    if !svc_clone.has_pending_migrations() {
                        break;
                    }
                    if svc_clone.try_run_migrations().await {
                        tracing::info!(
                            "Infinite Memory deferred migrations resolved by background retry"
                        );
                        break;
                    }
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
        // Try to acquire the execution lock
        if self.migrations_pending.load(Ordering::Acquire) == false {
            return false;
        }

        match opencode_mem_storage::pg_storage::infinite_memory::run_infinite_memory_migrations(
            &self.pool,
        )
        .await
        {
            Ok(()) => {
                self.migrations_pending.store(false, Ordering::Release);
                tracing::info!("Infinite Memory deferred migrations completed successfully");
                true
            }
            Err(e) => {
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
        let result = self
            .guarded(|| {
                opencode_mem_storage::pg_storage::infinite_memory::store_infinite_event(
                    &self.pool,
                    event.clone(),
                )
            })
            .await;
        result
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
        let result = self
            .guarded(|| {
                opencode_mem_storage::pg_storage::infinite_memory::create_5min_summary(
                    &self.pool,
                    &events,
                    &summary,
                    entities.as_ref(),
                )
            })
            .await;
        result
    }

    pub async fn run_compression_pipeline(&self) -> Result<u32> {
        pipeline::run_compression_pipeline(&self.pool, &self.llm).await
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
        let result = self
            .guarded(|| {
                opencode_mem_storage::pg_storage::infinite_memory::create_hour_summary(
                    &self.pool,
                    &summaries,
                    &content,
                    entities.as_ref(),
                )
            })
            .await;
        result
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
        let result = self
            .guarded(|| {
                opencode_mem_storage::pg_storage::infinite_memory::create_day_summary(
                    &self.pool,
                    &summaries,
                    &content,
                    entities.as_ref(),
                )
            })
            .await;
        result
    }

    pub async fn compress_summaries(&self, summaries: &[InfiniteSummary]) -> Result<String> {
        compression::compress_summaries(&self.llm, summaries).await
    }

    pub async fn run_full_compression(&self) -> Result<(u32, u32, u32)> {
        pipeline::run_full_compression(&self.pool, &self.llm).await
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
                        match opencode_mem_storage::pg_storage::infinite_memory::run_infinite_memory_migrations(
                            &this.pool,
                        )
                        .await
                        {
                            Ok(()) => {
                                tracing::info!(
                                    "Infinite Memory deferred migrations completed successfully"
                                );
                            }
                            Err(e) => {
                                this.migrations_pending.store(true, Ordering::Release);
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
                        match opencode_mem_storage::pg_storage::infinite_memory::run_infinite_memory_migrations(
                            &this.pool,
                        )
                        .await
                        {
                            Ok(()) => {
                                tracing::info!(
                                    "Infinite Memory deferred migrations completed successfully"
                                );
                            }
                            Err(e) => {
                                this.migrations_pending.store(true, Ordering::Release);
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
