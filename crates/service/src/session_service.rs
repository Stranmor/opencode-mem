use std::sync::Arc;

use opencode_mem_core::{Observation, Session, SessionStatus};
use opencode_mem_llm::LlmClient;
use opencode_mem_storage::Storage;

pub struct SessionService {
    storage: Arc<Storage>,
    llm: Arc<LlmClient>,
}

impl SessionService {
    #[must_use]
    pub const fn new(storage: Arc<Storage>, llm: Arc<LlmClient>) -> Self {
        Self { storage, llm }
    }

    pub fn init_session(&self, session: Session) -> anyhow::Result<Session> {
        self.storage.save_session(&session)?;
        Ok(session)
    }

    pub async fn complete_session(&self, session_id: &str) -> anyhow::Result<Option<String>> {
        let observations = self.storage.get_session_observations(session_id)?;
        let summary = if observations.is_empty() {
            None
        } else {
            Some(self.generate_summary(&observations).await?)
        };
        self.storage.update_session_status_with_summary(
            session_id,
            SessionStatus::Completed,
            summary.as_deref(),
        )?;
        Ok(summary)
    }

    pub async fn generate_summary(&self, observations: &[Observation]) -> anyhow::Result<String> {
        Ok(self.llm.generate_session_summary(observations).await?)
    }

    pub async fn summarize_session(&self, session_id: &str) -> anyhow::Result<String> {
        let observations = self.storage.get_session_observations(session_id)?;
        let summary = self.llm.generate_session_summary(&observations).await?;
        self.storage.update_session_status_with_summary(
            session_id,
            SessionStatus::Completed,
            Some(&summary),
        )?;
        Ok(summary)
    }
}
