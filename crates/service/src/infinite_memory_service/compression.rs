use anyhow::Result;
use opencode_mem_core::{
    InfiniteSummary, StoredInfiniteEvent, SummaryEntities, strip_markdown_json,
};
use opencode_mem_llm::LlmClient;
use serde_json::Value;
use std::sync::OnceLock;

static MAX_CONTENT_CHARS: OnceLock<usize> = OnceLock::new();
static MAX_TOTAL_CHARS: OnceLock<usize> = OnceLock::new();
static MAX_EVENTS: OnceLock<usize> = OnceLock::new();

pub fn init_compression_config(
    max_content_chars: usize,
    max_total_chars: usize,
    max_events: usize,
) {
    let _ = MAX_CONTENT_CHARS.set(max_content_chars);
    let _ = MAX_TOTAL_CHARS.set(max_total_chars);
    let _ = MAX_EVENTS.set(max_events);
}

fn max_content_chars() -> Result<usize> {
    MAX_CONTENT_CHARS
        .get()
        .copied()
        .ok_or_else(|| anyhow::anyhow!("init_compression_config must be called before use"))
}

fn max_total_chars() -> Result<usize> {
    MAX_TOTAL_CHARS
        .get()
        .copied()
        .ok_or_else(|| anyhow::anyhow!("init_compression_config must be called before use"))
}

fn max_events() -> Result<usize> {
    MAX_EVENTS
        .get()
        .copied()
        .ok_or_else(|| anyhow::anyhow!("init_compression_config must be called before use"))
}

/// Truncate string values inside a `serde_json::Value` in-place.
///
/// This prevents JSON corruption: instead of serializing to string and
/// cutting off closing braces/quotes, we truncate the *content* of
/// individual text fields before serialization, keeping the JSON
/// structure valid.
fn truncate_json_values(value: &mut Value, max_chars: usize) {
    match value {
        Value::String(s) => {
            let truncated = opencode_mem_core::truncate(s, max_chars);
            if truncated.len() < s.len() {
                let mut owned = truncated.to_owned();
                owned.push('…');
                *s = owned;
            }
        }
        Value::Array(arr) => {
            for item in arr {
                truncate_json_values(item, max_chars);
            }
        }
        Value::Object(map) => {
            for (_key, val) in map.iter_mut() {
                truncate_json_values(val, max_chars);
            }
        }
        _ => {}
    }
}

pub async fn compress_events(
    llm: &LlmClient,
    events: &[StoredInfiniteEvent],
) -> Result<(String, Option<SummaryEntities>)> {
    if events.is_empty() {
        return Ok((String::new(), None));
    }

    let max_ev = max_events()?;
    if events.len() > max_ev {
        anyhow::bail!(
            "compress_events called with {} events, max allowed: {}",
            events.len(),
            max_ev
        );
    }

    let max_content = max_content_chars()?;
    let max_total = max_total_chars()?;
    let mut events_text: Vec<String> = Vec::with_capacity(events.len());
    let mut total_chars = 0usize;

    for e in events {
        let mut content = e.content.clone();
        truncate_json_values(&mut content, max_content);
        let content_str = serde_json::to_string(&content).unwrap_or_default();
        let line = format!(
            "[{}] {}: {}",
            e.event_type,
            e.ts.format("%H:%M:%S"),
            content_str
        );
        total_chars = total_chars.saturating_add(line.chars().count());
        if total_chars > max_total {
            events_text.push(format!(
                "...({} more events truncated)",
                events.len().saturating_sub(events_text.len())
            ));
            break;
        }
        events_text.push(line);
    }

    let prompt = format!(
        r#"Проанализируй эти {} событий и верни JSON:
{{
  "summary": "Краткое описание на русском (2-3 предложения)",
  "entities": {{
    "files": ["список изменённых файлов"],
    "functions": ["упомянутые функции"],
    "libraries": ["внешние библиотеки"],
    "errors": ["типы ошибок"],
    "decisions": ["ключевые решения"]
  }}
}}

События:
{}"#,
        events.len(),
        events_text.join("\n")
    );

    let request = opencode_mem_llm::ChatRequest {
        model: llm.model().to_owned(),
        messages: vec![opencode_mem_llm::Message {
            role: "user".to_owned(),
            content: prompt,
        }],
        response_format: opencode_mem_llm::ResponseFormat {
            format_type: opencode_mem_llm::ResponseFormatType::JsonObject,
        },
        max_tokens: None,
    };

    let content = llm
        .chat_completion(&request)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send compression request: {}", e))?;

    let content = strip_markdown_json(&content);
    let parsed: serde_json::Value = serde_json::from_str(content).map_err(|e| {
        anyhow::anyhow!(
            "Failed to parse AI JSON response: {}. Content: {}",
            e,
            content
        )
    })?;

    let summary = parsed
        .get("summary")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("LLM returned response without summary field"))?
        .to_string();
    let entities: Option<SummaryEntities> = parsed
        .get("entities")
        .and_then(|e| serde_json::from_value(e.clone()).ok());

    Ok((summary, entities))
}

pub async fn compress_summaries(llm: &LlmClient, summaries: &[InfiniteSummary]) -> Result<String> {
    if summaries.is_empty() {
        return Ok(String::new());
    }

    let summaries_text: Vec<String> = summaries
        .iter()
        .map(|s| {
            format!(
                "[{} - {}] {}",
                s.ts_start.format("%H:%M"),
                s.ts_end.format("%H:%M"),
                s.content
            )
        })
        .collect();

    let prompt = format!(
        "Объедини эти {} сводок в одну краткую сводку на русском (2-3 предложения). \
         Сохрани ключевые факты, файлы, решения.\n\n{}",
        summaries.len(),
        summaries_text.join("\n\n")
    );

    let request = opencode_mem_llm::ChatRequest {
        model: llm.model().to_owned(),
        messages: vec![opencode_mem_llm::Message {
            role: "user".to_owned(),
            content: prompt,
        }],
        response_format: opencode_mem_llm::ResponseFormat {
            format_type: opencode_mem_llm::ResponseFormatType::Text,
        },
        max_tokens: Some(1500),
    };

    llm.chat_completion(&request)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send summary compression request: {}", e))
}
