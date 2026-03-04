use opencode_mem_core::{sanitize_input, Observation, ObservationType};

#[test]
fn test_post_llm_title_filtering() {
    let mut obs = Observation::builder(
        "id".into(),
        "tool".into(),
        ObservationType::Discovery,
        "Found <private>secret123</private>".into(),
    )
    .build();

    obs.title = sanitize_input(&obs.title);
    assert_eq!(obs.title.trim(), "Found");
}
