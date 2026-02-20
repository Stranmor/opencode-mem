//! Pure merge computation for observation deduplication.
//!
//! Extracts the field-level merge logic used by the PostgreSQL
//! `merge_into_existing` implementation so the computation lives in one place
//! (SPOT) and the storage backend only handles DB transactions.

use super::dedup::{union_dedup, union_dedup_concepts};
use super::{Concept, DiscoveryTokens, NoiseLevel, Observation, ObservationType, PromptNumber};

/// Result of merging two observations.
///
/// Contains all fields that change during a merge. The storage layer applies
/// these values to the existing row via UPDATE.
#[derive(Debug, Clone)]
pub struct MergeResult {
    pub facts: Vec<String>,
    pub keywords: Vec<String>,
    pub files_read: Vec<String>,
    pub files_modified: Vec<String>,
    pub concepts: Vec<Concept>,
    pub narrative: Option<String>,
    pub subtitle: Option<String>,
    pub noise_level: NoiseLevel,
    pub noise_reason: Option<String>,
    pub prompt_number: Option<PromptNumber>,
    pub discovery_tokens: Option<DiscoveryTokens>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub title: String,
    pub observation_type: ObservationType,
}

/// Compute merged fields for two observations.
/// Does not mutate them directly; returns a `MergeResult` struct with the resolved fields.
pub fn compute_merge(existing: &Observation, newer: &Observation) -> MergeResult {
    let facts = union_dedup(&existing.facts, &newer.facts);
    let keywords = union_dedup(&existing.keywords, &newer.keywords);
    let files_read = union_dedup(&existing.files_read, &newer.files_read);
    let files_modified = union_dedup(&existing.files_modified, &newer.files_modified);
    let concepts = union_dedup_concepts(&existing.concepts, &newer.concepts);

    let narrative = pick_longer_optional(&existing.narrative, &newer.narrative);
    let subtitle = pick_longer_optional(&existing.subtitle, &newer.subtitle);

    // NoiseLevel Ord: Critical(0) < High(1) < ... < Negligible(4)
    // min picks the most important (lowest discriminant = highest importance)
    let noise_level = std::cmp::min(existing.noise_level, newer.noise_level);

    let noise_reason = newer.noise_reason.clone().or_else(|| existing.noise_reason.clone());
    let prompt_number = newer.prompt_number.or(existing.prompt_number);
    let discovery_tokens = newer.discovery_tokens.or(existing.discovery_tokens);

    let created_at = existing.created_at.max(newer.created_at);

    // Context-aware compressions might refine the title or change type based on new context
    let title = newer.title.clone();
    let observation_type = newer.observation_type;

    MergeResult {
        facts,
        keywords,
        files_read,
        files_modified,
        concepts,
        narrative,
        subtitle,
        noise_level,
        noise_reason,
        prompt_number,
        discovery_tokens,
        created_at,
        title,
        observation_type,
    }
}

/// Pick the longer of two optional strings. Prefer `existing` when lengths are equal.
fn pick_longer_optional(existing: &Option<String>, newer: &Option<String>) -> Option<String> {
    match (existing, newer) {
        (Some(e), Some(n)) if n.len() > e.len() => Some(n.clone()),
        (None, Some(n)) => Some(n.clone()),
        (Some(e), _) => Some(e.clone()),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::{Observation, ObservationType};

    fn make_obs(title: &str) -> Observation {
        Observation::builder(
            format!("id-{title}"),
            "session-1".to_owned(),
            ObservationType::Discovery,
            title.to_owned(),
        )
        .build()
    }

    #[test]
    fn merge_picks_longer_narrative() {
        let mut existing = make_obs("test");
        existing.narrative = Some("short".to_owned());
        let mut newer = make_obs("test2");
        newer.narrative = Some("much longer narrative".to_owned());

        let result = compute_merge(&existing, &newer);
        assert_eq!(result.narrative.as_deref(), Some("much longer narrative"));
    }

    #[test]
    fn merge_prefers_existing_when_equal_length() {
        let mut existing = make_obs("test");
        existing.narrative = Some("aaaa".to_owned());
        let mut newer = make_obs("test2");
        newer.narrative = Some("bbbb".to_owned());

        let result = compute_merge(&existing, &newer);
        assert_eq!(result.narrative.as_deref(), Some("aaaa"));
    }

    #[test]
    fn merge_union_dedup_facts() {
        let mut existing = make_obs("test");
        existing.facts = vec!["a".to_owned(), "b".to_owned()];
        let mut newer = make_obs("test2");
        newer.facts = vec!["b".to_owned(), "c".to_owned()];

        let result = compute_merge(&existing, &newer);
        assert_eq!(result.facts, vec!["a", "b", "c"]);
    }

    #[test]
    fn merge_picks_most_important_noise_level() {
        let mut existing = make_obs("test");
        existing.noise_level = NoiseLevel::Low;
        let mut newer = make_obs("test2");
        newer.noise_level = NoiseLevel::Critical;

        let result = compute_merge(&existing, &newer);
        assert_eq!(result.noise_level, NoiseLevel::Critical);
    }

    #[test]
    fn merge_picks_later_timestamp() {
        let mut existing = make_obs("test");
        let t1 = Utc::now();
        existing.created_at = t1;
        let mut newer = make_obs("test2");
        let t2 = t1 + chrono::Duration::seconds(10);
        newer.created_at = t2;

        let result = compute_merge(&existing, &newer);
        assert_eq!(result.created_at, t2);
    }

    #[test]
    fn merge_none_none_narrative() {
        let existing = make_obs("test");
        let newer = make_obs("test2");

        let result = compute_merge(&existing, &newer);
        assert!(result.narrative.is_none());
    }

    #[test]
    fn merge_none_some_narrative() {
        let existing = make_obs("test");
        let mut newer = make_obs("test2");
        newer.narrative = Some("new text".to_owned());

        let result = compute_merge(&existing, &newer);
        assert_eq!(result.narrative.as_deref(), Some("new text"));
    }
}
