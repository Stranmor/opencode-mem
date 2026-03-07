use std::sync::Arc;

use opencode_mem_core::{Observation, Session, SessionStatus};
use opencode_mem_llm::LlmClient;
use opencode_mem_storage::traits::{ObservationStore, SessionStore, SummaryStore};
use opencode_mem_storage::{StorageBackend, StorageError};

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

    pub(crate) fn with_cb<T>(&self, result: Result<T, StorageError>) -> Result<T, ServiceError> {
        result.map_err(ServiceError::from)
    }

    pub async fn init_session(&self, session: Session) -> Result<Session, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.save_session(&session))
            .await;
        self.with_cb(result)?;
        Ok(session)
    }

    pub async fn get_session(&self, id: &str) -> Result<Option<Session>, ServiceError> {
        let result = self.storage.guarded(|| self.storage.get_session(id)).await;
        self.with_cb(result)
    }

    pub async fn get_session_observation_count(
        &self,
        session_id: &str,
    ) -> Result<usize, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.get_session_observation_count(session_id))
            .await;
        self.with_cb(result)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<bool, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.delete_session(session_id))
            .await;
        self.with_cb(result)
    }

    pub async fn get_session_by_content_id(
        &self,
        content_session_id: &str,
    ) -> Result<Option<Session>, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.get_session_by_content_id(content_session_id))
            .await;
        self.with_cb(result)
    }

    pub async fn close_stale_sessions(&self, max_age_hours: i64) -> Result<usize, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.close_stale_sessions(max_age_hours))
            .await;
        self.with_cb(result)
    }

    pub async fn complete_session(&self, session_id: &str) -> Result<Option<String>, ServiceError> {
        let observations = self
            .storage
            .guarded(|| self.storage.get_session_observations(session_id))
            .await;
        let observations = self.with_cb(observations)?;

        let summary = if observations.is_empty() {
            None
        } else {
            Some(self.generate_summary(&observations).await?)
        };

        let result = self
            .storage
            .guarded(|| {
                self.storage.update_session_status_with_summary(
                    session_id,
                    SessionStatus::Completed,
                    summary.as_deref(),
                )
            })
            .await;
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
        let observations = self
            .storage
            .guarded(|| self.storage.get_session_observations(session_id))
            .await;
        let observations = self.with_cb(observations)?;

        if observations.is_empty() {
            let result = self
                .storage
                .guarded(|| {
                    self.storage.update_session_status_with_summary(
                        session_id,
                        SessionStatus::Completed,
                        None,
                    )
                })
                .await;
            self.with_cb(result)?;
            return Ok("No observations in this session.".to_owned());
        }
        let summary = self.llm.generate_session_summary(&observations).await?;

        let result = self
            .storage
            .guarded(|| {
                self.storage.update_session_status_with_summary(
                    session_id,
                    SessionStatus::Completed,
                    Some(&summary),
                )
            })
            .await;
        self.with_cb(result)?;
        Ok(summary)
    }
}
