use std::sync::Arc;

use chrono::Utc;
use opencode_mem_core::{
    Observation, ProjectId, Session, SessionId, SessionStatus, SessionSummary,
};
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

    pub async fn generate_pending_summaries(&self, limit: usize) -> Result<usize, ServiceError> {
        let result = self
            .storage
            .guarded(|| self.storage.get_sessions_without_summaries(limit))
            .await;
        let sessions = self.with_cb(result)?;

        if sessions.is_empty() {
            return Ok(0);
        }

        let mut generated: usize = 0;
        for session in &sessions {
            let exists_result = self
                .storage
                .guarded(|| self.storage.get_summary(&session.session_id))
                .await;

            match self.with_cb(exists_result) {
                Ok(Some(_)) => continue,
                Ok(None) => {}
                Err(_) => continue,
            }

            let placeholder = SessionSummary::new(
                SessionId::from(session.session_id.clone()),
                ProjectId::new("processing"),
                None,
                None,
                Some("processing".to_owned()),
                None,
                None,
                None,
                Vec::new(),
                Vec::new(),
                None,
                None,
                Utc::now(),
            );

            let claim_result = self
                .storage
                .guarded(|| self.storage.save_summary(&placeholder))
                .await;

            if self.with_cb(claim_result).is_err() {
                continue;
            }

            let obs_result = self
                .storage
                .guarded(|| self.storage.get_session_observations(&session.session_id))
                .await;
            let mut observations = match self.with_cb(obs_result) {
                Ok(obs) => obs,
                Err(e) => {
                    tracing::warn!(
                        session_id = %session.session_id,
                        error = %e,
                        "Failed to fetch observations for session summary"
                    );
                    continue;
                }
            };

            if observations.len() < 2 {
                continue;
            }

            if observations.len() > 150 {
                tracing::warn!(
                    session_id = %session.session_id,
                    count = observations.len(),
                    "Session has too many observations, truncating to last 150 for summary"
                );
                let start_idx = observations.len() - 150;
                observations = observations.into_iter().skip(start_idx).collect();
            }

            let summary_text = match self.llm.generate_session_summary(&observations).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        session_id = %session.session_id,
                        error = %e,
                        "LLM session summary generation failed"
                    );
                    let _ = self
                        .storage
                        .guarded(|| self.storage.delete_summary(&session.session_id))
                        .await;
                    continue;
                }
            };

            if observations.len() < 2 {
                continue;
            }

            // Truncate to prevent LLM token overflow
            if observations.len() > 150 {
                tracing::warn!(
                    session_id = %session.session_id,
                    count = observations.len(),
                    "Session has too many observations, truncating to last 150 for summary"
                );
                let start_idx = observations.len() - 150;
                observations = observations.into_iter().skip(start_idx).collect();
            }

            let summary_text = match self.llm.generate_session_summary(&observations).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        session_id = %session.session_id,
                        error = %e,
                        "LLM session summary generation failed"
                    );
                    // Remove placeholder so it can be retried later
                    let _ = self
                        .storage
                        .guarded(|| self.storage.delete_summary(&session.session_id))
                        .await;
                    continue;
                }
            };

            let project = session
                .project
                .clone()
                .unwrap_or_else(|| ProjectId::new("unknown"));

            let summary = SessionSummary::new(
                SessionId::from(session.session_id.clone()),
                project,
                None,
                None,
                Some(summary_text),
                None,
                None,
                None,
                Vec::new(),
                Vec::new(),
                None,
                None,
                Utc::now(),
            );

            let result = self
                .storage
                .guarded(|| self.storage.save_summary(&summary))
                .await;
            if let Err(e) = self.with_cb(result) {
                tracing::warn!(
                    session_id = %session.session_id,
                    error = %e,
                    "Failed to store session summary"
                );
                continue;
            }

            tracing::info!(
                session_id = %session.session_id,
                observations = observations.len(),
                "Generated autonomous session summary"
            );
            generated = generated.saturating_add(1);
        }

        Ok(generated)
    }
}
