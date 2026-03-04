use std::sync::Arc;

use opencode_mem_core::{Observation, Session, SessionStatus};
use opencode_mem_llm::LlmClient;
use opencode_mem_storage::traits::{ObservationStore, SessionStore, SummaryStore};
use opencode_mem_storage::StorageBackend;

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

    pub async fn init_session(&self, session: Session) -> Result<Session, ServiceError> {
        self.storage.save_session(&session).await?;
        Ok(session)
    }

    pub async fn get_session(&self, id: &str) -> Result<Option<Session>, ServiceError> {
        Ok(self.storage.get_session(id).await?)
    }

    pub async fn get_session_observation_count(
        &self,
        session_id: &str,
    ) -> Result<usize, ServiceError> {
        Ok(self.storage.get_session_observation_count(session_id).await?)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<bool, ServiceError> {
        Ok(self.storage.delete_session(session_id).await?)
    }

    pub async fn get_session_by_content_id(
        &self,
        content_session_id: &str,
    ) -> Result<Option<Session>, ServiceError> {
        Ok(self.storage.get_session_by_content_id(content_session_id).await?)
    }

    pub async fn close_stale_sessions(&self, max_age_hours: i64) -> Result<usize, ServiceError> {
        Ok(self.storage.close_stale_sessions(max_age_hours).await?)
    }

    pub async fn complete_session(&self, session_id: &str) -> Result<Option<String>, ServiceError> {
        let observations = self.storage.get_session_observations(session_id).await?;
        let summary = if observations.is_empty() {
            None
        } else {
            Some(self.generate_summary(&observations).await?)
        };
        self.storage
            .update_session_status_with_summary(
                session_id,
                SessionStatus::Completed,
                summary.as_deref(),
            )
            .await?;
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
        let observations = self.storage.get_session_observations(session_id).await?;
        if observations.is_empty() {
            self.storage
                .update_session_status_with_summary(session_id, SessionStatus::Completed, None)
                .await?;
            return Ok("No observations in this session.".to_owned());
        }
        let summary = self.llm.generate_session_summary(&observations).await?;
        self.storage
            .update_session_status_with_summary(
                session_id,
                SessionStatus::Completed,
                Some(&summary),
            )
            .await?;
        Ok(summary)
    }
}
