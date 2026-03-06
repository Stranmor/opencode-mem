use std::env;

use crate::client::LlmClient;
use opencode_mem_core::{ObservationInput, SessionId, ToolOutput};

pub(super) fn create_client() -> Option<LlmClient> {
    let api_key = env::var("OPENCODE_MEM_API_KEY")
        .or_else(|_| env::var("ANTIGRAVITY_API_KEY"))
        .ok()?;
    Some(
        LlmClient::new(
            api_key,
            env::var("OPENCODE_MEM_API_URL")
                .unwrap_or_else(|_| "https://api.openai.com".to_owned()),
            env::var("OPENCODE_MEM_MODEL").unwrap_or_else(|_| "gpt-4o".to_owned()),
        )
        .ok()?,
    )
}

pub(super) fn make_input(tool: &str, title: &str, output: &str) -> ObservationInput {
    ObservationInput::new(
        tool.to_owned(),
        SessionId::from("test-session"),
        format!("test-call-{}", uuid::Uuid::new_v4()),
        ToolOutput::new(title.to_owned(), output.to_owned(), serde_json::Value::Null),
    )
}
