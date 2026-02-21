//! Content filtering for private tags and injected memory blocks.

use regex::Regex;
use std::sync::LazyLock;

/// Safely removes XML-like blocks while properly handling nesting.
/// Avoids O(N) allocations and regex limitations around nested structures.
fn strip_nested_blocks(text: &str, tag_prefix: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut depth: i32 = 0;

    let mut chars = text.chars().peekable();

    // We cannot use to_lowercase() up front because it breaks character count mapping
    // when characters expand (e.g. 'ß' -> 'ss', 'İ' -> 'i' + dot).
    // Instead we do a case-insensitive check on the fly.
    let open_prefix: Vec<char> = format!("<{tag_prefix}").chars().collect();
    let close_prefix: Vec<char> = format!("</{tag_prefix}").chars().collect();

    while let Some(&c) = chars.peek() {
        if c == '<' || c == 'p' || c == 'P' || c == 'm' || c == 'M' || c == '/' {
            // Check for opening tag
            let mut is_open = true;
            let mut peek_chars = chars.clone();
            for &expected_c in &open_prefix {
                if let Some(actual_c) = peek_chars.next() {
                    if actual_c.to_ascii_lowercase() != expected_c {
                        is_open = false;
                        break;
                    }
                } else {
                    is_open = false;
                    break;
                }
            }

            if is_open {
                let mut valid_tag = false;
                let mut scan_limit: usize = 500;
                let mut chars_to_consume = open_prefix.len();

                // Allow attributes, look for closing bracket.
                let scan_peek = peek_chars.clone();
                for ch in scan_peek {
                    if scan_limit == 0 {
                        break;
                    }
                    chars_to_consume = chars_to_consume.saturating_add(1);
                    if ch == '>' {
                        valid_tag = true;
                        break;
                    }
                    scan_limit = scan_limit.saturating_sub(1);
                }

                if valid_tag {
                    depth = depth.saturating_add(1);
                    // Advance main iterator past the tag
                    for _ in 0..chars_to_consume {
                        chars.next();
                    }
                    continue;
                }
            }

            // Check for closing tag
            if depth > 0 {
                let mut is_close = true;
                let mut peek_chars_close = chars.clone();
                for &expected_c in &close_prefix {
                    if let Some(actual_c) = peek_chars_close.next() {
                        if actual_c.to_ascii_lowercase() != expected_c {
                            is_close = false;
                            break;
                        }
                    } else {
                        is_close = false;
                        break;
                    }
                }

                if is_close {
                    let mut valid_tag = false;
                    let mut chars_to_consume = close_prefix.len();

                    let mut scan_limit: usize = 100;
                    let scan_peek = peek_chars_close.clone();
                    for ch in scan_peek {
                        if scan_limit == 0 {
                            break;
                        }
                        chars_to_consume = chars_to_consume.saturating_add(1);
                        if ch == '>' {
                            valid_tag = true;
                            break;
                        }
                        scan_limit = scan_limit.saturating_sub(1);
                    }

                    if valid_tag {
                        depth = depth.saturating_sub(1);
                        // Advance main iterator past the tag
                        for _ in 0..chars_to_consume {
                            chars.next();
                        }
                        continue;
                    }
                }
            }
        }

        // If not inside any block, keep the character
        if depth == 0 {
            result.push(c);
        }

        chars.next();
    }

    result
}

/// Regex for unclosed private tags (truncation safety).
#[expect(clippy::unwrap_used, reason = "static regex pattern is compile-time validated")]
static PRIVATE_UNCLOSED_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<private(?:>|\s[^>]*>).*$").unwrap());

/// Regex for orphaned closing private tags left after nested tag stripping.
#[expect(clippy::unwrap_used, reason = "static regex pattern is compile-time validated")]
static PRIVATE_ORPHAN_CLOSE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)</private>").unwrap());

/// Filters out content wrapped in `<private>...</private>` tags.
/// Handles both well-formed tags and unclosed tags.
pub fn filter_private_content(text: &str) -> String {
    let stripped = strip_nested_blocks(text, "private");
    let after_unclosed = PRIVATE_UNCLOSED_REGEX.replace_all(&stripped, "");
    PRIVATE_ORPHAN_CLOSE_REGEX.replace_all(&after_unclosed, "").into_owned()
}

/// Regex for unclosed memory tags (truncation safety).
/// Strips from opening tag to end-of-string when no closing tag exists.
#[expect(clippy::unwrap_used, reason = "static regex pattern is compile-time validated")]
static MEMORY_UNCLOSED_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<memory-[\w-]+(?:>|\s[^>]*>).*$").unwrap());

/// Regex for orphaned closing memory tags left after nested tag stripping.
/// When nested tags like `<memory-global><memory-project>...</memory-project></memory-global>`
/// are processed, the lazy `.*?` in `MEMORY_TAG_REGEX` strips the inner pair, leaving
/// `</memory-global>` as an orphan. This third pass catches those remnants.
#[expect(clippy::unwrap_used, reason = "static regex pattern is compile-time validated")]
static MEMORY_ORPHAN_CLOSE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)</memory-[\w-]+>").unwrap());

/// Strips injected memory blocks (`<memory-*>...</memory-*>`) from text.
///
/// Handles both well-formed tags and unclosed tags (e.g. from truncation).
pub fn filter_injected_memory(text: &str) -> String {
    let stripped = strip_nested_blocks(text, "memory-");
    let after_unclosed = MEMORY_UNCLOSED_REGEX.replace_all(&stripped, "");
    MEMORY_ORPHAN_CLOSE_REGEX.replace_all(&after_unclosed, "").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_private_simple() {
        let input = "Hello <private>secret</private> world";
        assert_eq!(filter_private_content(input), "Hello  world");
    }

    #[test]
    fn filter_private_multiline() {
        let input = "Start\n<private>\nSecret data\n</private>\nEnd";
        assert_eq!(filter_private_content(input), "Start\n\nEnd");
    }

    #[test]
    fn filter_private_case_insensitive() {
        let input = "Hello <PRIVATE>secret</PRIVATE> world";
        assert_eq!(filter_private_content(input), "Hello  world");
    }

    #[test]
    fn filter_private_multiple_tags() {
        let input = "A <private>x</private> B <private>y</private> C";
        assert_eq!(filter_private_content(input), "A  B  C");
    }

    #[test]
    fn filter_private_no_tags() {
        let input = "No private content here";
        assert_eq!(filter_private_content(input), "No private content here");
    }

    #[test]
    fn filter_private_empty_tag() {
        let input = "Hello <private></private> world";
        assert_eq!(filter_private_content(input), "Hello  world");
    }

    #[test]
    fn filter_private_nested_content() {
        let input = "Data <private>API_KEY=sk-12345\nPASSWORD=hunter2</private> end";
        assert_eq!(filter_private_content(input), "Data  end");
    }

    #[test]
    fn filter_private_unclosed_tag() {
        let input = "before <private>leaked secret content";
        let result = filter_private_content(input);
        assert_eq!(result, "before ");
    }

    #[test]
    fn filter_private_nested_leak() {
        let input = "<private> A <private> B </private> C </private> safe";
        let result = filter_private_content(input);
        assert_eq!(result, " safe");
    }

    #[test]
    fn filter_memory_global() {
        let input = "Normal text\n<memory-global>\n- [gotcha] Some memory\n- [decision] Another\n</memory-global>\nMore text";
        assert_eq!(filter_injected_memory(input), "Normal text\n\nMore text");
    }

    #[test]
    fn filter_memory_multiple_tags() {
        let input = "A <memory-global>x</memory-global> B <memory-project>y</memory-project> C";
        assert_eq!(filter_injected_memory(input), "A  B  C");
    }

    #[test]
    fn filter_memory_case_insensitive() {
        let input = "Hello <MEMORY-GLOBAL>data</MEMORY-GLOBAL> world";
        assert_eq!(filter_injected_memory(input), "Hello  world");
    }

    #[test]
    fn filter_memory_no_tags() {
        let input = "No memory tags here";
        assert_eq!(filter_injected_memory(input), "No memory tags here");
    }

    #[test]
    fn filter_memory_multiline_content() {
        let input = "Start\n<memory-global>\n- line 1\n- line 2\n- line 3\n</memory-global>\nEnd";
        assert_eq!(filter_injected_memory(input), "Start\n\nEnd");
    }

    #[test]
    fn filter_memory_preserves_private_tags() {
        let input = "A <private>secret</private> B <memory-global>mem</memory-global> C";
        assert_eq!(filter_injected_memory(input), "A <private>secret</private> B  C");
    }

    // ========================================================================
    // Regression tests: adversarial attack vectors against filter_injected_memory
    // ========================================================================

    // --- VULNERABILITY: Unclosed tags bypass filter entirely ---
    // An IDE plugin crash or malicious input can produce unclosed memory tags.
    // The regex requires a closing tag, so unclosed content leaks through unfiltered.
    #[test]
    fn filter_memory_unclosed_tag_stripped() {
        let input = "before <memory-global>leaked secret content";
        let result = filter_injected_memory(input);
        // Unclosed tags stripped to end-of-string (truncation safety)
        assert_eq!(result, "before ");
    }

    // --- VULNERABILITY: Tags with attributes bypass filter ---
    // If the IDE plugin or a future version adds attributes (class, id, data-*),
    // the regex fails to match because it expects `>` right after `[a-z]+`.
    #[test]
    fn filter_memory_tag_with_attributes_stripped() {
        let input = r#"before <memory-global class="injected">secret</memory-global> after"#;
        let result = filter_injected_memory(input);
        assert_eq!(result, "before  after");
    }

    #[test]
    fn filter_memory_tag_with_data_attribute_stripped() {
        let input = r#"<memory-project data-source="plugin">observations</memory-project> tail"#;
        let result = filter_injected_memory(input);
        assert_eq!(result, " tail");
    }

    // --- VULNERABILITY: Hyphenated suffixes bypass filter ---
    // `[a-z]+` stops at the first non-alpha character. Tags like <memory-global-v2>
    // or <memory-per-file> have hyphens in the suffix that break the match.
    #[test]
    fn filter_memory_hyphenated_suffix_matched() {
        let input = "<memory-global-v2>secret data</memory-global-v2> after";
        let result = filter_injected_memory(input);
        assert_eq!(result, " after");
    }

    #[test]
    fn filter_memory_multi_hyphen_suffix_matched() {
        let input = "<memory-per-file-cache>data</memory-per-file-cache>";
        let result = filter_injected_memory(input);
        assert_eq!(result, "");
    }

    // --- VULNERABILITY: Nested tags — inner stripped, outer tags leak ---
    // When memory tags are nested, the lazy `.*?` matches the inner pair,
    // leaving the outer opening and closing tags as orphaned text in output.
    #[test]
    fn filter_memory_nested_tags_partial_strip() {
        let input =
            "<memory-global><memory-project>inner secret</memory-project></memory-global> after";
        let result = filter_injected_memory(input);
        assert_eq!(result, " after");
    }

    #[test]
    fn filter_memory_nested_different_types() {
        let input = "head <memory-global>outer <memory-session>inner</memory-session> tail</memory-global> end";
        let result = filter_injected_memory(input);
        assert_eq!(result, "head  end");
    }

    // --- VULNERABILITY: Mismatched tag names still match ---
    // The regex doesn't enforce that open and close tag suffixes are identical.
    // <memory-foo>...</memory-bar> matches — could strip legitimate content.
    #[test]
    fn filter_memory_mismatched_tags_match() {
        let input = "<memory-foo>content</memory-bar> safe";
        let result = filter_injected_memory(input);
        assert_eq!(result, " safe");
    }

    // --- VULNERABILITY: Numeric/alphanumeric suffixes bypass ---
    // `[a-z]+` doesn't match digits. Tags like <memory-v2> bypass.
    #[test]
    fn filter_memory_numeric_suffix_matched() {
        let input = "<memory-v2>secret</memory-v2> rest";
        let result = filter_injected_memory(input);
        assert_eq!(result, " rest");
    }

    // --- VULNERABILITY: Whitespace inside tag bypasses ---
    #[test]
    fn filter_memory_whitespace_in_tag_stripped() {
        let input = "<memory-global >content</memory-global> after";
        let result = filter_injected_memory(input);
        assert_eq!(result, " after");
    }

    #[test]
    fn filter_memory_newline_in_tag_stripped() {
        let input = "<memory-global\n>content</memory-global> after";
        let result = filter_injected_memory(input);
        assert_eq!(result, " after");
    }

    // --- FALSE POSITIVE: Code discussions about memory tags get stripped ---
    #[test]
    fn filter_memory_code_discussion_false_positive() {
        let input = "The IDE uses <memory-global>...</memory-global> tags for injection.";
        let result = filter_injected_memory(input);
        // Legitimate discussion about the tag format gets stripped.
        // This is a false positive — the user was talking ABOUT the tags.
        assert_eq!(result, "The IDE uses  tags for injection.");
        // Note: This is inherently hard to fix without context awareness,
        // but it should be documented as a known false positive.
    }

    #[test]
    fn filter_memory_markdown_code_block_false_positive() {
        let input = "Example:\n```\n<memory-global>example data</memory-global>\n```\nEnd";
        let result = filter_injected_memory(input);
        // Content inside markdown code blocks gets stripped — false positive.
        // The regex has no awareness of code block boundaries.
        assert_eq!(result, "Example:\n```\n\n```\nEnd");
        // DESIRED: preserve content inside code blocks
        // assert_eq!(result, input);
    }

    // --- SAFETY: ReDoS resistance (Rust `regex` crate = guaranteed O(n)) ---
    #[test]
    fn filter_memory_large_content_no_redos() {
        // 1MB of content between tags — should complete in bounded time.
        let big_content = "x".repeat(1_000_000);
        let input = format!("<memory-global>{}</memory-global>", big_content);
        let start = std::time::Instant::now();
        let result = filter_injected_memory(&input);
        let elapsed = start.elapsed();
        assert_eq!(result, "");
        assert!(elapsed.as_secs() < 2, "Parser took {:?} — potential perf issue", elapsed);
    }

    #[test]
    fn filter_memory_large_content_no_match_no_redos() {
        // 1MB of content with unclosed tag — now stripped by unclosed regex
        let big_content = "x".repeat(1_000_000);
        let input = format!("<memory-global>{}", big_content);
        let start = std::time::Instant::now();
        let result = filter_injected_memory(&input);
        let elapsed = start.elapsed();
        assert_eq!(result, "");
        assert!(
            elapsed.as_secs() < 2,
            "Parser took {:?} on unclosed tag — potential perf issue",
            elapsed
        );
    }

    // --- SAFETY: Greedy matching across blocks (should be lazy) ---
    #[test]
    fn filter_memory_lazy_match_does_not_cross_blocks() {
        let input = "<memory-global>a</memory-global> KEEP THIS <memory-project>b</memory-project>";
        let result = filter_injected_memory(input);
        // Lazy .*? should match each block independently, preserving text between.
        assert_eq!(result, " KEEP THIS ");
    }

    // --- EDGE: Self-closing/empty tag variant ---
    #[test]
    fn filter_memory_empty_tag() {
        let input = "before <memory-global></memory-global> after";
        let result = filter_injected_memory(input);
        assert_eq!(result, "before  after");
    }

    // --- EDGE: Mixed case suffix ---
    #[test]
    fn filter_memory_mixed_case_suffix() {
        let input = "<Memory-Global>content</Memory-Global> rest";
        let result = filter_injected_memory(input);
        // (?i) flag makes this case-insensitive — should strip
        assert_eq!(result, " rest");
    }

    // --- VULNERABILITY: Tag name with underscore bypasses ---
    #[test]
    fn filter_memory_underscore_suffix_matched() {
        let input = "<memory-per_project>data</memory-per_project> rest";
        let result = filter_injected_memory(input);
        assert_eq!(result, " rest");
    }

    // --- EDGE: Multiple unclosed tags accumulate ---
    #[test]
    fn filter_memory_multiple_unclosed_tags_stripped() {
        let input = "<memory-global>leak1 <memory-project>leak2 <memory-session>leak3";
        let result = filter_injected_memory(input);
        assert_eq!(result, "");
    }

    // --- EDGE: Closing tag without opening tag ---
    #[test]
    fn filter_memory_orphaned_close_tag() {
        let input = "before </memory-global> after";
        let result = filter_injected_memory(input);
        // Orphaned close tags are now stripped by the third-pass regex
        assert_eq!(result, "before  after");
    }
}
