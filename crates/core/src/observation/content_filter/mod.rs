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

/// Standardized sanitization pipeline. Applies all required filters in the correct order.
///
/// Current pipeline:
/// 1. Remove injected memory tags (to prevent infinite context loops)
/// 2. Remove private content (to prevent leaking sensitive data)
pub fn sanitize_input(text: &str) -> String {
    let no_injected = filter_injected_memory(text);
    filter_private_content(&no_injected)
}

#[cfg(test)]
mod tests;
