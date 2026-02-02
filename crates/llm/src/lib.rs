//! LLM client for observation compression and summary generation
use anyhow::Result;
use chrono::Utc;
use opencode_mem_core::{Concept, Observation, ObservationInput, ObservationType};
use serde::{Deserialize, Serialize};

pub struct LlmClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    response_format: ResponseFormat,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

#[derive(Deserialize)]
struct ObservationJson {
    #[serde(rename = "type")]
    observation_type: String,
    title: String,
    subtitle: Option<String>,
    narrative: Option<String>,
    #[serde(default)]
    facts: Vec<String>,
    #[serde(default)]
    concepts: Vec<String>,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    keywords: Vec<String>,
}

impl LlmClient {
    pub fn new(api_key: String, base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url,
        }
    }

    pub async fn compress_to_observation(
        &self,
        id: &str,
        input: &ObservationInput,
    ) -> Result<Observation> {
        let prompt = format!(
            r#"Analyze this tool execution and extract a structured observation.

Tool: {}
Output Title: {}
Output Content: {}

Return JSON with these fields:
- type: one of "bugfix", "feature", "refactor", "change", "discovery", "decision"
- title: concise title (max 100 chars)
- subtitle: optional one-line context
- narrative: optional 2-3 sentence explanation of what happened and why
- facts: array of specific facts learned (file paths, function names, decisions)
- concepts: array from ["how-it-works", "why-it-exists", "what-changed", "problem-solution", "gotcha", "pattern", "trade-off"]
- files: array of file paths mentioned
- keywords: array of 5-10 semantic keywords for search (technologies, patterns, concepts)"#,
            input.tool,
            input.output.title,
            truncate(&input.output.output, 2000),
        );

        let request = ChatRequest {
            model: "gemini-3-pro-high".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
            response_format: ResponseFormat {
                format_type: "json_object".to_string(),
            },
        };

        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            anyhow::bail!("API error {}: {}", status, body);
        }

        let chat_response: ChatResponse = serde_json::from_str(&body).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse response: {} - body: {}",
                e,
                &body[..body.len().min(500)]
            )
        })?;

        let first_choice = chat_response
            .choices
            .first()
            .ok_or_else(|| anyhow::anyhow!("API returned empty choices array"))?;

        let content = strip_markdown_json(&first_choice.message.content);
        let obs_json: ObservationJson = serde_json::from_str(content).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse observation JSON: {} - content: {}",
                e,
                &content[..content.len().min(300)]
            )
        })?;

        // Adapt Observation construction
        // Use 'files' for both read and modified as we don't distinguish them in the prompt yet
        Ok(Observation {
            id: id.to_string(),
            session_id: input.session_id.clone(),
            observation_type: parse_observation_type(&obs_json.observation_type),
            title: obs_json.title,
            subtitle: obs_json.subtitle,
            narrative: obs_json.narrative,
            facts: obs_json.facts,
            concepts: obs_json.concepts.iter().map(|s| parse_concept(s)).collect(),
            files_read: obs_json.files.clone(),
            files_modified: obs_json.files,
            keywords: obs_json.keywords,
            prompt_number: None,
            discovery_tokens: None,
            created_at: Utc::now(),
        })
    }

    pub async fn generate_session_summary(&self, observations: &[Observation]) -> Result<String> {
        if observations.is_empty() {
            return Ok("No observations in this session.".to_string());
        }

        let obs_text: String = observations
            .iter()
            .map(|o| {
                format!(
                    "- [{}] {}: {}",
                    o.observation_type.as_str(),
                    o.title,
                    o.subtitle.as_deref().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"Summarize this coding session based on the observations below.
Write 2-3 sentences highlighting key accomplishments and decisions.

Observations:
{}

Return JSON: {{"summary": "..."}}"#,
            obs_text
        );

        let request = ChatRequest {
            model: "gemini-3-pro-high".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
            response_format: ResponseFormat {
                format_type: "json_object".to_string(),
            },
        };

        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        let body = response.text().await?;
        let chat_response: ChatResponse = serde_json::from_str(&body)?;
        let first_choice = chat_response
            .choices
            .first()
            .ok_or_else(|| anyhow::anyhow!("API returned empty choices array for session summary"))?;
        let content = strip_markdown_json(&first_choice.message.content);

        #[derive(Deserialize)]
        struct SummaryJson {
            summary: String,
        }
        let summary: SummaryJson = serde_json::from_str(content).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse session summary: {} - content: {}",
                e,
                &content[..content.len().min(300)]
            )
        })?;
        Ok(summary.summary)
    }
}

fn parse_observation_type(s: &str) -> ObservationType {
    use std::str::FromStr;
    ObservationType::from_str(s).unwrap_or(ObservationType::Change)
}

fn parse_concept(s: &str) -> Concept {
    match s.to_lowercase().as_str() {
        "how-it-works" => Concept::HowItWorks,
        "why-it-exists" => Concept::WhyItExists,
        "what-changed" => Concept::WhatChanged,
        "problem-solution" => Concept::ProblemSolution,
        "gotcha" => Concept::Gotcha,
        "pattern" => Concept::Pattern,
        "trade-off" => Concept::TradeOff,
        _ => Concept::WhatChanged,
    }
}

fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

fn strip_markdown_json(content: &str) -> &str {
    let trimmed = content.trim();
    if trimmed.starts_with("```json") {
        trimmed
            .strip_prefix("```json")
            .and_then(|s| s.strip_suffix("```"))
            .map(|s| s.trim())
            .unwrap_or(trimmed)
    } else if trimmed.starts_with("```") {
        trimmed
            .strip_prefix("```")
            .and_then(|s| s.strip_suffix("```"))
            .map(|s| s.trim())
            .unwrap_or(trimmed)
    } else {
        trimmed
    }
}
