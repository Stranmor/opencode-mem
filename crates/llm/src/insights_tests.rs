use super::*;

#[test]
fn missing_noise_level_defaults_to_medium() {
    let json = r#"{
  "insights": [
    {
      "type": "decision",
      "title": "T",
      "description": "D",
      "files": []
    }
  ]
}"#;

    let parsed: InsightsResponse = serde_json::from_str(json).expect("valid insights JSON");
    assert_eq!(parsed.insights.len(), 1);
    assert_eq!(parsed.insights.first().expect("should have one insight").noise_level, "medium");

    let observations = insights_to_observations(parsed.insights, "s", "p");
    assert_eq!(observations.len(), 1);
}

#[test]
fn negligible_noise_level_filtered_case_insensitive() {
    let insights = vec![InsightJson {
        insight_type: "decision".to_owned(),
        title: "T".to_owned(),
        description: "D".to_owned(),
        files: vec![],
        noise_level: "NEGLIGIBLE".to_owned(),
    }];

    let observations = insights_to_observations(insights, "s", "p");
    assert!(observations.is_empty());
}

#[test]
fn llm_returns_empty_list_ok() {
    let json = r#"{"insights": []}"#;
    let parsed: InsightsResponse = serde_json::from_str(json).expect("valid insights JSON");
    let observations = insights_to_observations(parsed.insights, "s", "p");
    assert!(observations.is_empty());
}

#[test]
fn invalid_noise_level_values_are_kept() {
    let insights = vec![InsightJson {
        insight_type: "decision".to_owned(),
        title: "T".to_owned(),
        description: "D".to_owned(),
        files: vec![],
        noise_level: "banana".to_owned(),
    }];

    let observations = insights_to_observations(insights, "s", "p");
    assert_eq!(observations.len(), 1);
}
