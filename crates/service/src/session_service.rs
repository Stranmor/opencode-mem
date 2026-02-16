use std::process::Command;
use std::sync::Arc;

use opencode_mem_core::{Observation, Session, SessionStatus};
use opencode_mem_llm::LlmClient;
use opencode_mem_storage::traits::{ObservationStore, SessionStore, SummaryStore};
use opencode_mem_storage::StorageBackend;

use crate::observation_service::ObservationService;

pub struct SessionService {
    storage: Arc<StorageBackend>,
    llm: Arc<LlmClient>,
    observation_service: Arc<ObservationService>,
}

impl SessionService {
    #[must_use]
    pub const fn new(
        storage: Arc<StorageBackend>,
        llm: Arc<LlmClient>,
        observation_service: Arc<ObservationService>,
    ) -> Self {
        Self { storage, llm, observation_service }
    }

    pub async fn init_session(&self, session: Session) -> anyhow::Result<Session> {
        self.storage.save_session(&session).await?;
        Ok(session)
    }

    pub async fn get_session(&self, id: &str) -> anyhow::Result<Option<Session>> {
        self.storage.get_session(id).await
    }

    pub async fn get_session_observation_count(&self, session_id: &str) -> anyhow::Result<usize> {
        self.storage.get_session_observation_count(session_id).await
    }

    pub async fn delete_session(&self, session_id: &str) -> anyhow::Result<bool> {
        self.storage.delete_session(session_id).await
    }

    pub async fn get_session_by_content_id(
        &self,
        content_session_id: &str,
    ) -> anyhow::Result<Option<Session>> {
        self.storage.get_session_by_content_id(content_session_id).await
    }

    pub async fn close_stale_sessions(&self, max_age_hours: i64) -> anyhow::Result<usize> {
        self.storage.close_stale_sessions(max_age_hours).await
    }

    pub async fn complete_session(&self, session_id: &str) -> anyhow::Result<Option<String>> {
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

    pub async fn generate_summary(&self, observations: &[Observation]) -> anyhow::Result<String> {
        Ok(self.llm.generate_session_summary(observations).await?)
    }

    pub async fn summarize_session(
        &self,
        session_id: &str,
        content_session_id: &str,
    ) -> anyhow::Result<String> {
        let observations = self.storage.get_session_observations(content_session_id).await?;
        if observations.is_empty() {
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

    pub async fn summarize_session_from_export(
        &self,
        content_session_id: &str,
    ) -> anyhow::Result<Vec<Observation>> {
        let output = Command::new("opencode").args(["export", content_session_id]).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("opencode export failed: {stderr}");
        }

        let session_json = String::from_utf8(output.stdout)?;

        let session = self.storage.get_session_by_content_id(content_session_id).await?;
        let (session_id, project_path) = if let Some(s) = session {
            (s.id, s.project)
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            let new_session = Session::new(
                id.clone(),
                content_session_id.to_owned(),
                None,
                String::new(),
                None,
                chrono::Utc::now(),
                None,
                SessionStatus::Active,
                0,
            );
            self.storage.save_session(&new_session).await?;
            (id, String::new())
        };

        let observations = self
            .llm
            .extract_insights_from_session(&session_json, &project_path, &session_id)
            .await?;

        for obs in &observations {
            self.observation_service.save_observation(obs).await?;
        }

        self.storage
            .update_session_status_with_summary(&session_id, SessionStatus::Completed, None)
            .await?;

        Ok(observations)
    }
}
