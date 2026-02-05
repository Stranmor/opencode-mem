//! JSON utility functions shared across crates.

/// Strip markdown code block wrappers from JSON content.
///
/// Handles `` ```json ... ``` ``, `` ``` ... ``` ``, and other language identifiers.
#[must_use]
pub fn strip_markdown_json(content: &str) -> &str {
    let trimmed = content.trim();
    if trimmed.starts_with("```") && trimmed.ends_with("```") {
        let without_prefix = trimmed.strip_prefix("```").unwrap_or(trimmed);
        let without_suffix = without_prefix.strip_suffix("```").unwrap_or(without_prefix);
        return without_suffix
            .split_once('\n')
            .map_or_else(|| return without_suffix.trim(), |(_, rest)| return rest.trim());
    }
    return trimmed;
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
}
