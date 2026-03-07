//! Tests for `observation_embedding_text` — pure function tests.

#![allow(clippy::unwrap_used, reason = "Unwraps are safe in tests")]

use opencode_mem_core::{ObservationType, observation_embedding_text};

#[test]
fn test_observation_embedding_text() {
    let obs = opencode_mem_core::Observation::builder(
        "obs-emb-txt".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "MyTitle".to_owned(),
    )
    .narrative("narrative part")
    .facts(vec!["fact-a".to_owned(), "fact-b".to_owned()])
    .build();

    let text = observation_embedding_text(&obs);

    // Format: "{title} {narrative} {facts joined by space}"
    assert_eq!(text, "MyTitle narrative part fact-a fact-b");
}

#[test]
fn test_observation_embedding_text_no_narrative_no_facts() {
    let obs = opencode_mem_core::Observation::builder(
        "obs-emb-txt-2".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "TitleOnly".to_owned(),
    )
    .build();

    let text = observation_embedding_text(&obs);

    // Implementation: format!("{} {} {}", title, "", "")
    // Produces "TitleOnly  " (two trailing spaces).
    // This is a known artifact of the format! macro when narrative and facts are empty.
    // The test validates actual behavior, not ideal behavior.
    assert_eq!(text, "TitleOnly  ");
}

#[test]
fn test_observation_embedding_text_narrative_only_no_facts() {
    let obs = opencode_mem_core::Observation::builder(
        "obs-emb-narr-only".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Title".to_owned(),
    )
    .narrative("some narrative")
    .build();

    let text = observation_embedding_text(&obs);
    // facts is empty → join("") = "", so trailing space after narrative.
    assert_eq!(text, "Title some narrative ");
}

#[test]
fn test_observation_embedding_text_facts_only_no_narrative() {
    let obs = opencode_mem_core::Observation::builder(
        "obs-emb-facts-only".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "Title".to_owned(),
    )
    .facts(vec!["fact1".to_owned()])
    .build();

    let text = observation_embedding_text(&obs);
    // narrative is None → unwrap_or("") → empty string, then space, then facts.
    assert_eq!(text, "Title  fact1");
}

#[test]
fn test_observation_embedding_text_unicode_content() {
    let obs = opencode_mem_core::Observation::builder(
        "obs-emb-unicode".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "🦀 Rust 日本語".to_owned(),
    )
    .narrative("narrative with émojis 👍")
    .facts(vec!["fact: Ü".to_owned()])
    .build();

    let text = observation_embedding_text(&obs);
    assert_eq!(text, "🦀 Rust 日本語 narrative with émojis 👍 fact: Ü");
}

#[test]
fn test_observation_embedding_text_many_facts() {
    let facts: Vec<String> = (0..100).map(|i| format!("fact-{i}")).collect();
    let obs = opencode_mem_core::Observation::builder(
        "obs-emb-many".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        "T".to_owned(),
    )
    .narrative("N")
    .facts(facts)
    .build();

    let text = observation_embedding_text(&obs);
    assert!(text.starts_with("T N fact-0"));
    assert!(text.ends_with("fact-99"));
    // Verify all 100 facts are present
    for i in 0..100 {
        assert!(
            text.contains(&format!("fact-{i}")),
            "missing fact-{i} in embedding text"
        );
    }
}

#[test]
fn test_observation_embedding_text_empty_title() {
    // Edge case: empty title. The builder allows it.
    let obs = opencode_mem_core::Observation::builder(
        "obs-emb-empty-title".to_owned(),
        "session-1".to_owned(),
        ObservationType::Discovery,
        String::new(), // empty title
    )
    .narrative("narrative")
    .facts(vec!["f".to_owned()])
    .build();

    let text = observation_embedding_text(&obs);
    assert_eq!(text, " narrative f");
}
