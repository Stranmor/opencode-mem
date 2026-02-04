//! JSON utility functions shared across crates.

/// Strip markdown code block wrappers from JSON content.
///
/// Handles both `\`\`\`json ... \`\`\`` and plain `\`\`\` ... \`\`\`` wrappers.
pub fn strip_markdown_json(content: &str) -> &str {
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
}
