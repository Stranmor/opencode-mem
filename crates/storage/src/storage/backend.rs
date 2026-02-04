use anyhow::Result;
use async_trait::async_trait;
use opencode_mem_core::{Observation, SearchResult, Session, StorageBackend};

use super::Storage;

#[async_trait]
impl StorageBackend for Storage {
    async fn save_observation(&self, obs: &Observation) -> Result<()> {
        let storage = self.clone();
        let obs = obs.clone();
        tokio::task::spawn_blocking(move || storage.save_observation(&obs)).await?
    }

    async fn get_observation(&self, id: &str) -> Result<Option<Observation>> {
        let storage = self.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || storage.get_by_id(&id)).await?
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let storage = self.clone();
        let query = query.to_string();
        tokio::task::spawn_blocking(move || storage.search(&query, limit)).await?
    }

    async fn get_recent(&self, limit: usize) -> Result<Vec<SearchResult>> {
        let storage = self.clone();
        tokio::task::spawn_blocking(move || storage.get_recent(limit)).await?
    }

    async fn save_session(&self, session: &Session) -> Result<()> {
        let storage = self.clone();
        let session = session.clone();
        tokio::task::spawn_blocking(move || storage.save_session(&session)).await?
    }

    async fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let storage = self.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || storage.get_session(&id)).await?
    }
}
