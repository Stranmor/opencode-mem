//! Observation struct and its builder pattern.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{Concept, NoiseLevel, ObservationType};

/// Structured observation of a coding activity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Observation {
    /// Unique identifier
    pub id: String,
    /// Session this observation belongs to
    pub session_id: String,
    /// Project this observation belongs to
    pub project: Option<String>,
    /// Type of observation
    pub observation_type: ObservationType,
    /// Concise title (max 100 chars)
    pub title: String,
    /// Optional one-line context
    pub subtitle: Option<String>,
    /// 2-3 sentence explanation of what happened and why
    pub narrative: Option<String>,
    /// Specific facts learned (file paths, function names, decisions)
    pub facts: Vec<String>,
    /// Semantic concepts for categorization
    pub concepts: Vec<Concept>,
    /// File paths mentioned or modified
    pub files_read: Vec<String>,
    /// File paths modified
    pub files_modified: Vec<String>,
    /// Semantic keywords for search
    pub keywords: Vec<String>,
    /// Prompt number within session
    pub prompt_number: Option<u32>,
    /// Token count for ROI tracking
    pub discovery_tokens: Option<u32>,
    /// Signal vs noise classification (Critical = must show, Negligible = hide by default)
    #[serde(default)]
    pub noise_level: NoiseLevel,
    /// Why this noise level was assigned
    pub noise_reason: Option<String>,
    /// When this observation was created
    pub created_at: DateTime<Utc>,
}

impl Observation {
    /// Returns a new builder with the required fields set.
    #[must_use]
    pub fn builder(
        id: String,
        session_id: String,
        observation_type: ObservationType,
        title: String,
    ) -> ObservationBuilder {
        ObservationBuilder::new(id, session_id, observation_type, title)
    }
}

/// Builder for constructing [`Observation`] instances.
#[derive(Debug, Clone)]
pub struct ObservationBuilder {
    id: String,
    session_id: String,
    observation_type: ObservationType,
    title: String,
    project: Option<String>,
    subtitle: Option<String>,
    narrative: Option<String>,
    facts: Vec<String>,
    concepts: Vec<Concept>,
    files_read: Vec<String>,
    files_modified: Vec<String>,
    keywords: Vec<String>,
    prompt_number: Option<u32>,
    discovery_tokens: Option<u32>,
    noise_level: NoiseLevel,
    noise_reason: Option<String>,
    created_at: DateTime<Utc>,
}

impl ObservationBuilder {
    #[must_use]
    fn new(
        id: String,
        session_id: String,
        observation_type: ObservationType,
        title: String,
    ) -> Self {
        Self {
            id,
            session_id,
            observation_type,
            title,
            project: None,
            subtitle: None,
            narrative: None,
            facts: Vec::new(),
            concepts: Vec::new(),
            files_read: Vec::new(),
            files_modified: Vec::new(),
            keywords: Vec::new(),
            prompt_number: None,
            discovery_tokens: None,
            noise_level: NoiseLevel::default(),
            noise_reason: None,
            created_at: Utc::now(),
        }
    }

    #[must_use]
    pub fn project(mut self, project: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self
    }

    #[must_use]
    pub fn maybe_project(mut self, project: Option<String>) -> Self {
        self.project = project;
        self
    }

    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    #[must_use]
    pub fn maybe_subtitle(mut self, subtitle: Option<String>) -> Self {
        self.subtitle = subtitle;
        self
    }

    #[must_use]
    pub fn narrative(mut self, narrative: impl Into<String>) -> Self {
        self.narrative = Some(narrative.into());
        self
    }

    #[must_use]
    pub fn maybe_narrative(mut self, narrative: Option<String>) -> Self {
        self.narrative = narrative;
        self
    }

    #[must_use]
    pub fn facts(mut self, facts: Vec<String>) -> Self {
        self.facts = facts;
        self
    }

    #[must_use]
    pub fn concepts(mut self, concepts: Vec<Concept>) -> Self {
        self.concepts = concepts;
        self
    }

    #[must_use]
    pub fn files_read(mut self, files_read: Vec<String>) -> Self {
        self.files_read = files_read;
        self
    }

    #[must_use]
    pub fn files_modified(mut self, files_modified: Vec<String>) -> Self {
        self.files_modified = files_modified;
        self
    }

    #[must_use]
    pub fn keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords;
        self
    }

    #[must_use]
    pub fn prompt_number(mut self, prompt_number: u32) -> Self {
        self.prompt_number = Some(prompt_number);
        self
    }

    #[must_use]
    pub fn maybe_prompt_number(mut self, prompt_number: Option<u32>) -> Self {
        self.prompt_number = prompt_number;
        self
    }

    #[must_use]
    pub fn discovery_tokens(mut self, discovery_tokens: u32) -> Self {
        self.discovery_tokens = Some(discovery_tokens);
        self
    }

    #[must_use]
    pub fn maybe_discovery_tokens(mut self, discovery_tokens: Option<u32>) -> Self {
        self.discovery_tokens = discovery_tokens;
        self
    }

    #[must_use]
    pub fn noise_level(mut self, noise_level: NoiseLevel) -> Self {
        self.noise_level = noise_level;
        self
    }

    #[must_use]
    pub fn noise_reason(mut self, noise_reason: impl Into<String>) -> Self {
        self.noise_reason = Some(noise_reason.into());
        self
    }

    #[must_use]
    pub fn maybe_noise_reason(mut self, noise_reason: Option<String>) -> Self {
        self.noise_reason = noise_reason;
        self
    }

    #[must_use]
    pub fn created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = created_at;
        self
    }

    #[must_use]
    pub fn build(self) -> Observation {
        Observation {
            id: self.id,
            session_id: self.session_id,
            project: self.project,
            observation_type: self.observation_type,
            title: self.title,
            subtitle: self.subtitle,
            narrative: self.narrative,
            facts: self.facts,
            concepts: self.concepts,
            files_read: self.files_read,
            files_modified: self.files_modified,
            keywords: self.keywords,
            prompt_number: self.prompt_number,
            discovery_tokens: self.discovery_tokens,
            noise_level: self.noise_level,
            noise_reason: self.noise_reason,
            created_at: self.created_at,
        }
    }
}
