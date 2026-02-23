use crate::client::truncate;
use opencode_mem_core::{strip_markdown_json, Concept};
use std::str::FromStr;

#[test]
fn test_truncate_within_limit() {
    assert_eq!(truncate("hello", 10), "hello");
}

#[test]
fn test_truncate_at_limit() {
    assert_eq!(truncate("hello", 5), "hello");
}

#[test]
fn test_truncate_exceeds_limit() {
    assert_eq!(truncate("hello world", 5), "hello");
}

#[test]
fn test_truncate_unicode_boundary() {
    // "privet" in cyrillic: \u043f\u0440\u0438\u0432\u0435\u0442
    let s = "\u{043f}\u{0440}\u{0438}\u{0432}\u{0435}\u{0442}";
    let result = truncate(s, 4);
    assert!(result.len() <= 4);
}

#[test]
fn test_truncate_empty() {
    assert_eq!(truncate("", 10), "");
}

#[test]
fn test_strip_markdown_json_clean() {
    assert_eq!(strip_markdown_json(r#"{"key": "value"}"#), r#"{"key": "value"}"#);
}

#[test]
fn test_strip_markdown_json_with_fence() {
    let input = "```json\n{\"key\": \"value\"}\n```";
    assert_eq!(strip_markdown_json(input), r#"{"key": "value"}"#);
}

#[test]
fn test_strip_markdown_json_with_generic_fence() {
    let input = "```\n{\"key\": \"value\"}\n```";
    assert_eq!(strip_markdown_json(input), r#"{"key": "value"}"#);
}

#[test]
fn test_strip_markdown_json_with_whitespace() {
    let input = "  \n```json\n{\"key\": \"value\"}\n```  \n";
    assert_eq!(strip_markdown_json(input), r#"{"key": "value"}"#);
}

#[test]
fn test_parse_concept_valid() {
    assert_eq!(Concept::from_str("how-it-works").ok(), Some(Concept::HowItWorks));
    assert_eq!(Concept::from_str("pattern").ok(), Some(Concept::Pattern));
    assert_eq!(Concept::from_str("gotcha").ok(), Some(Concept::Gotcha));
    assert_eq!(Concept::from_str("trade-off").ok(), Some(Concept::TradeOff));
}

#[test]
fn test_parse_concept_case_insensitive() {
    assert_eq!(Concept::from_str("HOW-IT-WORKS").ok(), Some(Concept::HowItWorks));
    assert_eq!(Concept::from_str("Pattern").ok(), Some(Concept::Pattern));
}

#[test]
fn test_parse_concept_invalid() {
    assert_eq!(Concept::from_str("unknown").ok(), None);
    assert_eq!(Concept::from_str("").ok(), None);
    assert_eq!(Concept::from_str("random-string").ok(), None);
}
