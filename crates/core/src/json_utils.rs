//! JSON utility functions shared across crates.

/// Strip markdown code block wrappers from JSON content.
///
/// Handles LLM preamble text before the code block (e.g. "Here is the summary:\n```json\n{...}\n```")
/// by finding the FIRST ``` and LAST ``` in the input and extracting content between them.
#[must_use]
pub fn strip_markdown_json(content: &str) -> &str {
    let trimmed = content.trim();

    let Some(open_pos) = trimmed.find("```") else {
        return trimmed;
    };
    // Find the LAST occurrence of ```
    let Some(close_pos) = trimmed.rfind("```") else {
        return trimmed;
    };
    // open and close must be different positions (not the same ```)
    if close_pos <= open_pos {
        return trimmed;
    }

    // Content between opening ``` (skip past the ```) and closing ```
    let after_open = &trimmed[open_pos.saturating_add(3)..close_pos];

    // Skip the language identifier on the first line (e.g. "json", "json5", " json")
    after_open.split_once('\n').map_or_else(|| after_open.trim(), |(_, rest)| rest.trim())
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
}
