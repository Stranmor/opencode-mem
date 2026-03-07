//! JSON utility functions shared across crates.

/// Strip markdown code block wrappers from JSON content.
///
/// Handles LLM preamble text before the code block (e.g. "Here is the summary:\n```json\n{...}\n```")
/// by finding the FIRST ``` and LAST ``` in the input and extracting content between them.
#[must_use]
pub fn strip_markdown_json(content: &str) -> &str {
    let trimmed = content.trim();

    // If it already looks like raw JSON (starts with { or [), return as-is.
    // This prevents incorrectly stripping JSON that contains nested code blocks.
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return trimmed;
    }

    let Some(open_pos) = trimmed.find("```") else {
        return trimmed;
    };

    let content_start = open_pos.saturating_add(3);

    // Find the NEXT occurrence of ``` after the opening block.
    // This isolates the actual JSON block even if subsequent prose contains code blocks.
    let Some(close_pos) = trimmed[content_start..].find("```") else {
        return trimmed;
    };
    let close_pos = close_pos.saturating_add(content_start);

    // Content between opening ``` (skip past the ```) and closing ```
    let after_open = &trimmed[content_start..close_pos];

    // Skip the language identifier on the first line (e.g. "json", "json5", " json")
    after_open
        .split_once('\n')
        .map_or_else(|| after_open.trim(), |(_, rest)| rest.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_json_block() {
        let input = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(strip_markdown_json(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_strip_plain_block() {
        let input = "```\n{\"key\": \"value\"}\n```";
        assert_eq!(strip_markdown_json(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_no_block() {
        let input = "{\"key\": \"value\"}";
        assert_eq!(strip_markdown_json(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_whitespace() {
        let input = "  ```json\n{\"key\": \"value\"}\n```  ";
        assert_eq!(strip_markdown_json(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_json5_block() {
        let input = "```json5\n{\"key\": \"value\"}\n```";
        assert_eq!(strip_markdown_json(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_space_before_lang() {
        let input = "``` json\n{\"key\": \"value\"}\n```";
        assert_eq!(strip_markdown_json(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_preamble_text_before_code_block() {
        let input = "Here is the summary:\n```json\n{\"key\": \"value\"}\n```";
        assert_eq!(strip_markdown_json(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_preamble_and_trailing_text() {
        let input = "Sure! Here you go:\n```json\n{\"key\": \"value\"}\n```\nHope this helps!";
        assert_eq!(strip_markdown_json(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_preamble_plain_block() {
        let input = "Result:\n```\n{\"key\": \"value\"}\n```";
        assert_eq!(strip_markdown_json(input), "{\"key\": \"value\"}");
    }

    #[test]
    fn test_overlapping_backticks_no_panic() {
        let input = "````";
        assert_eq!(strip_markdown_json(input), "````");

        let input2 = "`````";
        assert_eq!(strip_markdown_json(input2), "`````");
    }
}
