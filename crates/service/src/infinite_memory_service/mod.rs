mod compression;
mod pipeline;
mod queries;

use opencode_mem_core::{InfiniteSummary, RawInfiniteEvent, StoredInfiniteEvent, SummaryEntities};
use opencode_mem_llm::LlmClient;
use opencode_mem_storage::CircuitBreaker;

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
    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }

    pub async fn try_run_migrations(&self) -> bool {
        if self
            .migrations_pending
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }

        match opencode_mem_storage::pg_storage::infinite_memory::run_infinite_memory_migrations(
            &self.pool,
        )
        .await
        {
            Ok(()) => {
                tracing::info!("Infinite Memory deferred migrations completed successfully");
                true
            }
            Err(e) => {
                self.migrations_pending.store(true, Ordering::Release);
                tracing::warn!("Infinite Memory deferred migration attempt failed: {e}");
                false
            }
        }
    }

    #[must_use]
    pub fn has_pending_migrations(&self) -> bool {
        self.migrations_pending.load(Ordering::Acquire)
    }

    fn record_result<T>(&self, result: &Result<T>) {
        if result.is_ok() {
            let recovered = self.circuit_breaker.record_success();
            if recovered && self.migrations_pending.load(Ordering::Acquire) {
                let this = self.clone();
                tokio::spawn(async move {
                    let _ = this.try_run_migrations().await;
                });
            }
        } else if let Err(e) = result {
            let is_transient = Self::is_transient_error(e);
            let is_relation_missing = Self::is_missing_relation_error(e);

            if is_transient {
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

            if is_relation_missing && self.migrations_pending.load(Ordering::Acquire) {
                let this = self.clone();
                tokio::spawn(async move {
                    let _ = this.try_run_migrations().await;
                });
            }
        }
    }

    fn is_transient_error(err: &anyhow::Error) -> bool {
        if let Some(sqlx_err) = err.downcast_ref::<sqlx::Error>() {
            return matches!(
                sqlx_err,
                sqlx::Error::PoolTimedOut
                    | sqlx::Error::PoolClosed
                    | sqlx::Error::WorkerCrashed
                    | sqlx::Error::Io(_)
            ) || matches!(sqlx_err, sqlx::Error::Database(db_err)
            if db_err.code().as_deref().is_some_and(|c|
                c.starts_with("08") || c.starts_with("53") || c.starts_with("57")
            ));
        }

        let msg = err.to_string();
        msg.contains("connection refused")
            || msg.contains("connection reset")
            || msg.contains("broken pipe")
            || msg.contains("pool timed out")
            || msg.contains("PoolTimedOut")
            || msg.contains("timed out while waiting")
    }

    fn is_missing_relation_error(err: &anyhow::Error) -> bool {
        let msg = err.to_string();
        msg.contains("relation") && msg.contains("does not exist")
    }

    pub async fn store_event(&self, event: RawInfiniteEvent) -> Result<i64> {
        let result = opencode_mem_storage::pg_storage::infinite_memory::store_infinite_event(
            &self.pool, event,
        )
        .await
        .map_err(anyhow::Error::from);
        self.record_result(&result);
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
    ) -> Result<i64> {
        let result = opencode_mem_storage::pg_storage::infinite_memory::create_5min_summary(
            &self.pool, events, summary, entities,
        )
        .await
        .map_err(anyhow::Error::from);
        self.record_result(&result);
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
    ) -> Result<i64> {
        let result = opencode_mem_storage::pg_storage::infinite_memory::create_hour_summary(
            &self.pool, summaries, content, entities,
        )
        .await
        .map_err(anyhow::Error::from);
        self.record_result(&result);
        result
    }

    pub async fn create_day_summary(
        &self,
        summaries: &[InfiniteSummary],
        content: &str,
        entities: Option<&SummaryEntities>,
    ) -> Result<i64> {
        let result = opencode_mem_storage::pg_storage::infinite_memory::create_day_summary(
            &self.pool, summaries, content, entities,
        )
        .await
        .map_err(anyhow::Error::from);
        self.record_result(&result);
        result
    }

    pub async fn compress_summaries(&self, summaries: &[InfiniteSummary]) -> Result<String> {
        compression::compress_summaries(&self.llm, summaries).await
    }

    pub async fn run_full_compression(&self) -> Result<(u32, u32, u32)> {
        pipeline::run_full_compression(&self.pool, &self.llm).await
    }
}
