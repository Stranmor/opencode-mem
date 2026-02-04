use anyhow::Result;
use async_trait::async_trait;
use opencode_mem_core::{Observation, ObservationType, SearchResult, Session, StorageBackend};

use crate::{EventType, InfiniteMemory, RawEvent, StoredEvent};

#[async_trait]
impl StorageBackend for InfiniteMemory {
    async fn save_observation(&self, obs: &Observation) -> Result<()> {
        let event = RawEvent {
            session_id: obs.session_id.clone(),
            project: obs.project.clone(),
            event_type: EventType::Tool,
            content: serde_json::json!({
                "observation_id": obs.id,
                "type": obs.observation_type.as_str(),
                "title": obs.title,
                "subtitle": obs.subtitle,
                "narrative": obs.narrative,
                "facts": obs.facts,
                "concepts": obs.concepts,
                "keywords": obs.keywords,
            }),
            files: obs
                .files_read
                .iter()
                .chain(obs.files_modified.iter())
                .cloned()
                .collect(),
            tools: vec!["observation".to_string()],
        };
        self.store_event(event).await?;
        Ok(())
    }

    async fn get_observation(&self, id: &str) -> Result<Option<Observation>> {
        let events = self.search(&format!("observation_id:{}", id), 1).await?;
        if let Some(event) = events.first() {
            Ok(Some(stored_event_to_observation(event)))
        } else {
            Ok(None)
        }
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let events = self.search(query, limit as i64).await?;
        Ok(events
            .into_iter()
            .map(|e| SearchResult {
                id: e.id.to_string(),
                title: extract_title(&e.content),
                subtitle: extract_subtitle(&e.content),
                observation_type: extract_observation_type(&e.content),
                score: 1.0,
            })
            .collect())
    }

    async fn get_recent(&self, limit: usize) -> Result<Vec<SearchResult>> {
        let events = self.get_recent(limit as i64).await?;
        Ok(events
            .into_iter()
            .map(|e| SearchResult {
                id: e.id.to_string(),
                title: extract_title(&e.content),
                subtitle: extract_subtitle(&e.content),
                observation_type: extract_observation_type(&e.content),
                score: 1.0,
            })
            .collect())
    }

    async fn save_session(&self, _session: &Session) -> Result<()> {
        Ok(())
    }

    async fn get_session(&self, _id: &str) -> Result<Option<Session>> {
        Ok(None)
    }
}

fn stored_event_to_observation(event: &StoredEvent) -> Observation {
    let content = &event.content;
    Observation {
        id: content
            .get("observation_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&event.id.to_string())
            .to_string(),
        session_id: event.session_id.clone(),
        project: event.project.clone(),
        observation_type: extract_observation_type(content),
        title: extract_title(content),
        subtitle: extract_subtitle(content),
        narrative: content
            .get("narrative")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        facts: content
            .get("facts")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        concepts: vec![],
        files_read: event.files.clone(),
        files_modified: vec![],
        keywords: content
            .get("keywords")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        prompt_number: None,
        discovery_tokens: None,
        created_at: event.ts,
    }
}

fn extract_title(content: &serde_json::Value) -> String {
    content
        .get("title")
        .and_then(|v| v.as_str())
        .or_else(|| content.get("text").and_then(|v| v.as_str()))
        .unwrap_or("Untitled")
        .chars()
        .take(100)
        .collect()
}

fn extract_subtitle(content: &serde_json::Value) -> Option<String> {
    content
        .get("subtitle")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn extract_observation_type(content: &serde_json::Value) -> ObservationType {
    content
        .get("type")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(ObservationType::Change)
}
