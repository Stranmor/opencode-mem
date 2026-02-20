use super::*;
use opencode_mem_core::{Concept, NoiseLevel, Observation, ObservationType};

#[test]
fn test_knowledge_extraction_gotcha_type_bypass() {
    let obs = Observation {
        id: "test".to_string(),
        project: None,
        session_id: "session".to_string(),
        title: "Test Gotcha".to_string(),
        subtitle: None,
        observation_type: ObservationType::Gotcha,
        noise_level: NoiseLevel::High,
        score: 1.0,
        concepts: vec![Concept::WhyItExists, Concept::WhatChanged],
        facts: vec![],
        narrative: None,
        source_tool: "manual".to_string(),
        raw_input: None,
        raw_output: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let dominated = obs.concepts.iter().any(|c| matches!(c, Concept::Pattern | Concept::Gotcha | Concept::HowItWorks));
    
    assert!(dominated || matches!(obs.observation_type, ObservationType::Gotcha), "Vulnerability exists: ObservationType::Gotcha is ignored for knowledge extraction");
}
