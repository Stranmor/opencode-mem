use opencode_mem_core::{Observation, ObservationType, filter_private_content, filter_injected_memory};

#[test]
fn test_post_llm_title_filtering() {
    let mut obs = Observation::builder(
        "id".into(),
        "tool".into(),
        ObservationType::Discovery,
        "Found <private>secret123</private>".into(),
    ).build();

    obs.title = filter_private_content(&filter_injected_memory(&obs.title));
    assert_eq!(obs.title.trim(), "Found");
}
