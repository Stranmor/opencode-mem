use std::fmt::Write as _;

use chrono::Utc;
use opencode_mem_core::{Concept, Error, NoiseLevel, Observation, ObservationType, Result};
use serde::Deserialize;

use crate::ai_types::{ChatRequest, Message, ResponseFormat};
use crate::client::{truncate, LlmClient};

/// Single insight extracted from session analysis
#[derive(Debug, Deserialize)]
pub struct InsightJson {
    #[serde(rename = "type", default)]
    pub insight_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub files: Vec<String>,
}

/// LLM response containing extracted insights
#[derive(Debug, Deserialize)]
pub struct InsightsResponse {
    #[serde(default)]
    pub insights: Vec<InsightJson>,
}

fn format_session_for_llm(session_json: &serde_json::Value) -> String {
    let mut output = String::new();

    let Some(messages) = session_json.get("messages").and_then(|m| m.as_array()) else {
        return output;
    };

    for msg in messages {
        let role = msg
            .get("info")
            .and_then(|i| i.get("role"))
            .and_then(|r| r.as_str())
            .unwrap_or("unknown");

        _ = writeln!(output, "\n[{role}]");

        if let Some(parts) = msg.get("parts").and_then(|p| p.as_array()) {
            for part in parts {
                format_part(&mut output, part);
            }
        }
    }

    output
}

fn format_part(output: &mut String, part: &serde_json::Value) {
    let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match part_type {
        "text" => {
            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                output.push_str(text);
                output.push('\n');
            }
        },
        "tool-invocation" => {
            let name = part.get("name").and_then(|n| n.as_str()).unwrap_or("tool");
            _ = writeln!(output, "[Tool: {name}]");

            if let Some(input) = part.get("input") {
                let input_str = input.to_string();
                let preview = truncate(&input_str, 200);
                _ = writeln!(output, "Input: {preview}");
            }

            if let Some(out) = part.get("output") {
                let out_str = out.to_string();
                let preview = truncate(&out_str, 500);
                _ = writeln!(output, "Output: {preview}");
            }
        },
        _ => {},
    }
}

fn map_insight_type(insight_type: &str) -> ObservationType {
    match insight_type.to_lowercase().as_str() {
        "decision" => ObservationType::Decision,
        "gotcha" => ObservationType::Gotcha,
        "preference" => ObservationType::Preference,
        _ => ObservationType::Discovery,
    }
}

fn map_insight_concepts(insight_type: &str) -> Vec<Concept> {
    match insight_type.to_lowercase().as_str() {
        "decision" => vec![Concept::WhyItExists, Concept::TradeOff],
        "gotcha" => vec![Concept::Gotcha, Concept::ProblemSolution],
        "preference" => vec![Concept::Pattern],
        "discovery" => vec![Concept::HowItWorks],
        _ => vec![],
    }
}

fn insight_to_observation(
    insight: InsightJson,
    session_id: &str,
    project_path: &str,
) -> Observation {
    let obs_type = map_insight_type(&insight.insight_type);
    let concepts = map_insight_concepts(&insight.insight_type);

    Observation::new(
        uuid::Uuid::new_v4().to_string(),
        session_id.to_owned(),
        Some(project_path.to_owned()),
        obs_type,
        insight.title,
        Some(format!("[{}]", insight.insight_type)),
        Some(insight.description),
        vec![],
        concepts,
        insight.files,
        vec![],
        vec![],
        None,
        None,
        NoiseLevel::default(),
        None,
        Utc::now(),
    )
}

impl LlmClient {
    /// Extract project-specific insights from a full session JSON export.
    ///
    /// # Errors
    /// Returns `Error::LlmApi` if the API call fails or response parsing fails.
    pub async fn extract_insights_from_session(
        &self,
        session_json: &str,
        project_path: &str,
        session_id: &str,
    ) -> Result<Vec<Observation>> {
        let parsed: serde_json::Value = serde_json::from_str(session_json)
            .map_err(|e| Error::InvalidInput(format!("Invalid session JSON: {e}")))?;

        let formatted = format_session_for_llm(&parsed);

        if formatted.trim().is_empty() {
            return Ok(vec![]);
        }

        let insights_response = self.call_llm_for_insights(&formatted, project_path).await?;

        let observations: Vec<Observation> = insights_response
            .insights
            .into_iter()
            .map(|insight| insight_to_observation(insight, session_id, project_path))
            .collect();

        Ok(observations)
    }

    async fn call_llm_for_insights(
        &self,
        formatted: &str,
        project_path: &str,
    ) -> Result<InsightsResponse> {
        let prompt = build_insights_prompt(formatted, project_path);

        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![Message { role: "user".to_owned(), content: prompt }],
            response_format: ResponseFormat { format_type: "json_object".to_owned() },
        };

        let content = self.chat_completion(&request).await?;
        serde_json::from_str(&content).map_err(|e| {
            Error::LlmApi(format!(
                "Failed to parse insights JSON: {e} - content: {}",
                content.get(..300).unwrap_or(&content)
            ))
        })
    }
}

fn build_insights_prompt(formatted: &str, project_path: &str) -> String {
    format!(
        r#"You are analyzing a coding session to extract project-specific knowledge worth remembering.

## Session from project: {project_path}

<session>
{formatted}
</session>

## What to extract

Extract ONLY project-specific insights that would help a new developer (or future AI) working on THIS project:

1. **Decisions** - Architecture choices, library selections, design patterns chosen for THIS project
2. **Gotchas** - Project-specific bugs, quirks, workarounds discovered
3. **Preferences** - User's coding style preferences for THIS project
4. **Discoveries** - How specific parts of THIS codebase work

## What to SKIP

- Generic programming knowledge (async patterns, error handling basics)
- Standard library usage
- Common tool operations (git, cargo, npm basics)
- Anything a senior developer would already know

## Output format

Return JSON:
```json
{{
  "insights": [
    {{
      "type": "decision|gotcha|preference|discovery",
      "title": "Short title (5-10 words)",
      "description": "Detailed explanation",
      "files": ["path/to/relevant/file.rs"]
    }}
  ]
}}
```

If no project-specific insights found, return: {{"insights": []}}"#
    )
}
