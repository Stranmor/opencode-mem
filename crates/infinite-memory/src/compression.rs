//! LLM compression logic for events and summaries.

use crate::event_types::{StoredEvent, Summary, SummaryEntities};
use anyhow::Result;
use opencode_mem_core::strip_markdown_json;
use opencode_mem_llm::LlmClient;

fn max_content_chars() -> usize {
    opencode_mem_core::env_parse_with_default("OPENCODE_MEM_MAX_CONTENT_CHARS", 500)
}

fn max_total_chars() -> usize {
    opencode_mem_core::env_parse_with_default("OPENCODE_MEM_MAX_TOTAL_CHARS", 8000)
}

fn max_events() -> usize {
    opencode_mem_core::env_parse_with_default("OPENCODE_MEM_MAX_EVENTS", 200)
}

pub async fn compress_events(
    llm: &LlmClient,
    events: &[StoredEvent],
) -> Result<(String, Option<SummaryEntities>)> {
    if events.is_empty() {
        return Ok((String::new(), None));
    }

    let max_events = max_events();
    if events.len() > max_events {
        anyhow::bail!(
            "compress_events called with {} events, max allowed: {}",
            events.len(),
            max_events
        );
    }

    let mut events_text: Vec<String> = Vec::with_capacity(events.len());
    let mut total_chars = 0usize;

    for e in events {
        let content_str = serde_json::to_string(&e.content).unwrap_or_default();
        let max_content = max_content_chars();
        let truncated = if content_str.chars().count() > max_content {
            format!("{}...(truncated)", content_str.chars().take(max_content).collect::<String>())
        } else {
            content_str
        };
        let line = format!("[{}] {}: {}", e.event_type, e.ts.format("%H:%M:%S"), truncated);
        total_chars += line.len();
        if total_chars > max_total_chars() {
            events_text
                .push(format!("...({} more events truncated)", events.len() - events_text.len()));
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
        messages: vec![opencode_mem_llm::Message { role: "user".to_owned(), content: prompt }],
        response_format: opencode_mem_llm::ResponseFormat { format_type: "json_object".to_owned() },
        max_tokens: None,
    };

    let content = llm
        .chat_completion(&request)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send compression request: {}", e))?;

    let content = strip_markdown_json(&content);
    let parsed: serde_json::Value = serde_json::from_str(content).map_err(|e| {
        anyhow::anyhow!("Failed to parse AI JSON response: {}. Content: {}", e, content)
    })?;

    let summary = parsed["summary"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("LLM returned response without summary field"))?
        .to_string();
    let entities: Option<SummaryEntities> =
        parsed.get("entities").and_then(|e| serde_json::from_value(e.clone()).ok());

    Ok((summary, entities))
}

pub async fn compress_summaries(llm: &LlmClient, summaries: &[Summary]) -> Result<String> {
    if summaries.is_empty() {
        return Ok(String::new());
    }

    let summaries_text: Vec<String> = summaries
        .iter()
        .map(|s| {
            format!("[{} - {}] {}", s.ts_start.format("%H:%M"), s.ts_end.format("%H:%M"), s.content)
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
        messages: vec![opencode_mem_llm::Message { role: "user".to_owned(), content: prompt }],
        response_format: opencode_mem_llm::ResponseFormat { format_type: "text".to_owned() },
        max_tokens: Some(300),
    };

    llm.chat_completion(&request)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send summary compression request: {}", e))
}
