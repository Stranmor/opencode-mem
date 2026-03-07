use std::sync::Arc;

use opencode_mem_core::{Observation, Session, SessionStatus};
use opencode_mem_llm::LlmClient;
use opencode_mem_storage::StorageBackend;
use opencode_mem_storage::traits::{ObservationStore, SessionStore, SummaryStore};

use crate::ServiceError;

pub struct SessionService {
    storage: Arc<StorageBackend>,
    llm: Arc<LlmClient>,
}

impl SessionService {
    #[must_use]
    pub const fn new(storage: Arc<StorageBackend>, llm: Arc<LlmClient>) -> Self {
        Self { storage, llm }
    }

    pub fn circuit_breaker(&self) -> &opencode_mem_storage::CircuitBreaker {
        self.storage.circuit_breaker()
    }

    fn fast_fail_if_db_unavailable(&self) -> Result<(), ServiceError> {
        let cb = self.storage.circuit_breaker();
        if cb.should_allow() {
            Ok(())
        } else {
            Err(ServiceError::Storage(
                opencode_mem_storage::StorageError::Unavailable {
                    seconds_until_probe: cb.seconds_until_probe(),
                },
            ))
        }
    }

    pub(crate) fn with_cb<T>(&self, result: Result<T, ServiceError>) -> Result<T, ServiceError> {
        match &result {
            Ok(_) => {
                self.storage.circuit_breaker().record_success();
            }
            Err(e) if e.is_db_unavailable() => self.storage.circuit_breaker().record_failure(),
            Err(e) if self.storage.circuit_breaker().is_half_open() => {
                if e.is_db_unavailable() || e.is_transient() {
                    self.storage.circuit_breaker().record_failure();
                } else {
                    self.storage.circuit_breaker().record_success();
                }
            }
            Err(_) => {}
        }
        result
    }

    pub async fn init_session(&self, session: Session) -> Result<Session, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .save_session(&session)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result)?;
        Ok(session)
    }

    pub async fn get_session(&self, id: &str) -> Result<Option<Session>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .get_session(id)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result)
    }

    pub async fn get_session_observation_count(
        &self,
        session_id: &str,
    ) -> Result<usize, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .get_session_observation_count(session_id)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<bool, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .delete_session(session_id)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result)
    }

    pub async fn get_session_by_content_id(
        &self,
        content_session_id: &str,
    ) -> Result<Option<Session>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .get_session_by_content_id(content_session_id)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result)
    }

    pub async fn close_stale_sessions(&self, max_age_hours: i64) -> Result<usize, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .close_stale_sessions(max_age_hours)
            .await
            .map_err(ServiceError::from);
        self.with_cb(result)
    }

    pub async fn complete_session(&self, session_id: &str) -> Result<Option<String>, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let observations = self
            .storage
            .get_session_observations(session_id)
            .await
            .map_err(ServiceError::from);
        let observations = self.with_cb(observations)?;

        let summary = if observations.is_empty() {
            None
        } else {
            Some(self.generate_summary(&observations).await?)
        };

        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .update_session_status_with_summary(
                session_id,
                SessionStatus::Completed,
                summary.as_deref(),
            )
            .await
            .map_err(ServiceError::from);
        self.with_cb(result)?;
        Ok(summary)
    }

    pub async fn generate_summary(
        &self,
        observations: &[Observation],
    ) -> Result<String, ServiceError> {
        Ok(self.llm.generate_session_summary(observations).await?)
    }

    pub async fn summarize_session(
        &self,
        session_id: &str,
        _content_session_id: &str,
    ) -> Result<String, ServiceError> {
        self.fast_fail_if_db_unavailable()?;
        let observations = self
            .storage
            .get_session_observations(session_id)
            .await
            .map_err(ServiceError::from);
        let observations = self.with_cb(observations)?;

        if observations.is_empty() {
            self.fast_fail_if_db_unavailable()?;
            let result = self
                .storage
                .update_session_status_with_summary(session_id, SessionStatus::Completed, None)
                .await
                .map_err(ServiceError::from);
            self.with_cb(result)?;
            return Ok("No observations in this session.".to_owned());
        }
        let summary = self.llm.generate_session_summary(&observations).await?;

        self.fast_fail_if_db_unavailable()?;
        let result = self
            .storage
            .update_session_status_with_summary(
                session_id,
                SessionStatus::Completed,
                Some(&summary),
            )
            .await
            .map_err(ServiceError::from);
        self.with_cb(result)?;
        Ok(summary)
    }
}
