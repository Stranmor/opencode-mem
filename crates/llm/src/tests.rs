use crate::client::truncate;
use crate::observation::parse_concept;
use opencode_mem_core::{strip_markdown_json, Concept};

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
    let s = "привет";
    let result = truncate(s, 4);
    assert!(result.len() <= 4);
}

#[test]
fn test_truncate_empty() {
    assert_eq!(truncate("", 10), "");
}

#[test]
fn test_strip_markdown_json_clean() {
    assert_eq!(
        strip_markdown_json(r#"{"key": "value"}"#),
        r#"{"key": "value"}"#
    );
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
    assert_eq!(parse_concept("how-it-works"), Some(Concept::HowItWorks));
    assert_eq!(parse_concept("pattern"), Some(Concept::Pattern));
    assert_eq!(parse_concept("gotcha"), Some(Concept::Gotcha));
    assert_eq!(parse_concept("trade-off"), Some(Concept::TradeOff));
}

#[test]
fn test_parse_concept_case_insensitive() {
    assert_eq!(parse_concept("HOW-IT-WORKS"), Some(Concept::HowItWorks));
    assert_eq!(parse_concept("Pattern"), Some(Concept::Pattern));
}

#[test]
fn test_parse_concept_invalid() {
    assert_eq!(parse_concept("unknown"), None);
    assert_eq!(parse_concept(""), None);
    assert_eq!(parse_concept("random-string"), None);
}
