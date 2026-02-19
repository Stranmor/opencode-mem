use std::fmt::Write as _;

use opencode_mem_core::{Concept, Observation, ObservationType};
use serde::Deserialize;

use crate::ai_types::{ChatRequest, Message, ResponseFormat};
use crate::client::{LlmClient, truncate};
use crate::error::LlmError;

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
    #[serde(default = "default_medium")]
    pub noise_level: String,
}

fn default_medium() -> String {
    "medium".to_owned()
}

fn is_negligible_noise_level(noise_level: &str) -> bool {
    noise_level.trim().eq_ignore_ascii_case("negligible")
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
                let preview = truncate(text, 1000);
                output.push_str(preview);
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

    Observation::builder(
        uuid::Uuid::new_v4().to_string(),
        session_id.to_owned(),
        obs_type,
        insight.title,
    )
    .project(project_path)
    .subtitle(format!("[{}]", insight.insight_type))
    .maybe_narrative(Some(insight.description))
    .concepts(concepts)
    .files_read(insight.files)
    .build()
}

fn insights_to_observations(
    insights: Vec<InsightJson>,
    session_id: &str,
    project_path: &str,
) -> Vec<Observation> {
    insights
        .into_iter()
        .filter(|insight| !is_negligible_noise_level(&insight.noise_level))
        .map(|insight| insight_to_observation(insight, session_id, project_path))
        .collect()
}

impl LlmClient {
    /// Extract project-specific insights from a full session JSON export.
    ///
    /// # Errors
    /// Returns an error if the API call fails or response parsing fails.
    pub async fn extract_insights_from_session(
        &self,
        session_json: &str,
        project_path: &str,
        session_id: &str,
    ) -> Result<Vec<Observation>, LlmError> {
        let parsed: serde_json::Value = serde_json::from_str(session_json).map_err(|e| {
            LlmError::JsonParse { context: "session JSON input".to_owned(), source: e }
        })?;

        let formatted = format_session_for_llm(&parsed);

        if formatted.trim().is_empty() {
            return Ok(vec![]);
        }

        let insights_response = self.call_llm_for_insights(&formatted, project_path).await?;

        Ok(insights_to_observations(insights_response.insights, session_id, project_path))
    }

    async fn call_llm_for_insights(
        &self,
        formatted: &str,
        project_path: &str,
    ) -> Result<InsightsResponse, LlmError> {
        let prompt = build_insights_prompt(formatted, project_path);

        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![Message { role: "user".to_owned(), content: prompt }],
            response_format: ResponseFormat { format_type: "json_object".to_owned() },
        };

        let content = self.chat_completion(&request).await?;
        let clean_json = strip_markdown_json(&content);
        serde_json::from_str(&clean_json).map_err(|e| LlmError::JsonParse {
            context: format!("insights response (content: {})", truncate(&clean_json, 300)),
            source: e,
        })
    }
}

fn strip_markdown_json(content: &str) -> String {
    let content = content.trim();
    if let Some(stripped) = content.strip_prefix("```json") {
        if let Some(end) = stripped.rfind("```") {
            return stripped[..end].trim().to_owned();
        }
    }
    if let Some(stripped) = content.strip_prefix("```") {
        if let Some(end) = stripped.rfind("```") {
            return stripped[..end].trim().to_owned();
        }
    }
    content.to_owned()
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

Rate each insight's importance:
- "critical"/"high": Project-specific decisions, gotchas, unique discoveries
- "medium": Useful context (default)
- "low": Minor observations
- "negligible": Generic knowledge, trivial operations — SKIP these entirely

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
      "files": ["path/to/relevant/file.rs"],
      "noise_level": "critical|high|medium|low|negligible"
    }}
  ]
}}
```

If no project-specific insights found, return: {{"insights": []}}"#
    )
}

#[cfg(test)]
#[path = "insights_tests.rs"]
mod tests;
