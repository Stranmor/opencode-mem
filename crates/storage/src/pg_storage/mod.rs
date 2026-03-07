//! PostgreSQL storage backend using sqlx.
//!
//! Split into modular files by domain concern.

// Arithmetic in DB operations (pagination, counting) is bounded by DB limits
#![allow(
    clippy::arithmetic_side_effects,
    reason = "DB row counts and pagination are bounded by PostgreSQL limits"
)]
// Absolute paths in error handling are acceptable
#![allow(
    clippy::absolute_paths,
    reason = "std paths in error handling are clear"
)]

mod domain_parsers;
mod embeddings;
pub mod infinite_memory;
mod injections;
mod knowledge;
mod observation_delete;
mod observations;
mod pending;
mod prompts;
mod row_parsers;
mod search;
mod sessions;
mod stats;
mod summaries;

use crate::circuit_breaker::CircuitBreaker;
use crate::error::StorageError;
use opencode_mem_core::{
    PG_POOL_ACQUIRE_TIMEOUT_SECS, PG_POOL_IDLE_TIMEOUT_SECS, PG_POOL_MAX_CONNECTIONS,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::pg_migrations::run_pg_migrations;

pub(crate) use domain_parsers::{
    collect_skipping_corrupt, row_to_knowledge, row_to_pending_message, row_to_prompt,
    row_to_session, row_to_summary,
};
pub(crate) use row_parsers::{
    escape_like, parse_json_value, parse_pg_noise_level, parse_pg_observation_type,
    row_to_observation, row_to_search_result, usize_to_i64,
};

#[derive(Clone, Debug)]
pub struct PgStorage {
    pool: PgPool,
    circuit_breaker: Arc<CircuitBreaker>,
    migrations_pending: Arc<AtomicBool>,
}

impl PgStorage {
    pub fn pool(&self) -> PgPool {
        self.pool.clone()
    }

    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }

    pub async fn new(database_url: &str) -> Result<Self, StorageError> {
        let pool = PgPoolOptions::new()
            .max_connections(PG_POOL_MAX_CONNECTIONS)
            .acquire_timeout(std::time::Duration::from_secs(PG_POOL_ACQUIRE_TIMEOUT_SECS))
            .idle_timeout(std::time::Duration::from_secs(PG_POOL_IDLE_TIMEOUT_SECS))
            .test_before_acquire(true)
            .connect_lazy(database_url)?;

        // Try to run migrations — if DB is unavailable, log warning and continue.
        // Deferred migrations will run on first successful connection via recovery hook.
        let migrations_pending = match run_pg_migrations(&pool).await {
            Ok(()) => {
                tracing::info!("PgStorage initialized with migrations");
                false
            }
            Err(e) => {
                tracing::warn!(
                    "PgStorage initialized without migrations (DB may be unavailable): {e}"
                );
                true
            }
        };

        Ok(Self {
            pool,
            circuit_breaker: Arc::new(CircuitBreaker::new()),
            migrations_pending: Arc::new(AtomicBool::new(migrations_pending)),
        })
    }

    /// Attempt to run pending migrations. Safe to call repeatedly — idempotent.
    /// Returns `Ok(true)` if migrations ran, `Ok(false)` if DB unavailable or not needed.
    pub async fn try_run_migrations(&self) -> Result<bool, StorageError> {
        if self
            .migrations_pending
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(false);
        }

        match run_pg_migrations(&self.pool).await {
            Ok(()) => {
                tracing::info!("Deferred migrations completed successfully");
                Ok(true)
            }
            Err(e) => {
                self.migrations_pending.store(true, Ordering::Release);
                tracing::debug!("Deferred migration attempt failed (DB may still be down): {e}");
                Ok(false)
            }
        }
    }

    #[must_use]
    pub fn has_pending_migrations(&self) -> bool {
        self.migrations_pending.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn new_degraded(database_url: &str) -> Self {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_millis(1))
            .connect_lazy(database_url)
            .unwrap_or_else(|_| {
                PgPoolOptions::new()
                    .max_connections(1)
                    .acquire_timeout(std::time::Duration::from_millis(1))
                    .connect_lazy("postgres://localhost/nonexistent")
                    .expect("lazy pool creation should not fail")
            });
        let cb = CircuitBreaker::new();
        // Trip the circuit breaker so all operations return Unavailable
        cb.record_failure();
        cb.record_failure();
        cb.record_failure();
        Self {
            pool,
            circuit_breaker: Arc::new(cb),
            migrations_pending: Arc::new(AtomicBool::new(true)),
        }
    }

    #[allow(
        dead_code,
        reason = "CB guard infrastructure — used by guarded() and available for direct use"
    )]
    pub(crate) fn check_availability(&self) -> Result<(), StorageError> {
        if self.circuit_breaker.should_allow() {
            Ok(())
        } else {
            Err(StorageError::Unavailable {
                seconds_until_probe: self.circuit_breaker.seconds_until_probe(),
            })
        }
    }

    #[allow(dead_code, reason = "CB guard infrastructure — used by guarded()")]
    pub(crate) fn record_success(&self) {
        self.circuit_breaker.record_success();
    }

    #[allow(dead_code, reason = "CB guard infrastructure — used by guarded()")]
    pub(crate) fn record_failure_if_connection_error(&self, err: &StorageError) {
        if err.is_transient() || matches!(err, StorageError::Database(_)) {
            self.circuit_breaker.record_failure();
        }
    }

    pub async fn guarded<F, Fut, T>(&self, op_f: F) -> Result<T, StorageError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, StorageError>>,
    {
        self.check_availability()?;
        let result = op_f().await;
        match &result {
            Ok(_) => {
                let recovered = self.circuit_breaker.record_success();
                if recovered {
                    self.handle_recovery_static();
                }
            }
            Err(e) if e.is_unavailable() => {
                self.circuit_breaker.record_failure();
            }
            Err(e) if self.circuit_breaker.is_half_open() => {
                if e.is_unavailable() {
                    self.circuit_breaker.record_failure();
                } else {
                    let recovered = self.circuit_breaker.record_success();
                    if recovered {
                        self.handle_recovery_static();
                    }
                }
            }
            Err(_) => {}
        }
        result
    }

    pub fn handle_recovery_static(&self) {
        if self.has_pending_migrations() {
            let this = self.clone();
            tokio::spawn(async move {
                let _ = this.try_run_migrations().await;
            });
        }
    }
}

pub(crate) use search::utils::build_tsquery;

pub(crate) const SESSION_COLUMNS: &str =
    "id, content_session_id, memory_session_id, project, user_prompt,
     started_at, ended_at, status, prompt_counter";

pub(crate) const KNOWLEDGE_COLUMNS: &str =
    "id, knowledge_type, title, description, instructions, triggers,
     source_projects, source_observations, confidence, usage_count,
     last_used_at, created_at, updated_at, archived_at";

pub(crate) const SUMMARY_COLUMNS: &str =
    "id, ts_start, ts_end, session_id, project, content, event_count, entities";

pub(crate) const OBSERVATION_COLUMNS: &str = "id, session_id, project, observation_type, title, subtitle, narrative, facts, concepts, files_read, files_modified, keywords, prompt_number, discovery_tokens, noise_level, noise_reason, created_at";

pub(crate) const EVENT_COLUMNS: &str =
    "id, ts, session_id, project, event_type, content, files, tools, call_id";
