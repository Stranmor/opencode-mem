use super::super::*;

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
    assert_eq!(
        filter_injected_memory(input),
        "A <private>secret</private> B  C"
    );
}

// ========================================================================
// Regression tests: adversarial attack vectors against filter_injected_memory
// ========================================================================

#[test]
fn filter_memory_unclosed_tag_stripped() {
    let input = "before <memory-global>leaked secret content";
    let result = filter_injected_memory(input);
    assert_eq!(result, "before ");
}

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

#[test]
fn filter_memory_nested_tags_partial_strip() {
    let input =
        "<memory-global><memory-project>inner secret</memory-project></memory-global> after";
    let result = filter_injected_memory(input);
    assert_eq!(result, " after");
}

#[test]
fn filter_memory_nested_different_types() {
    let input =
        "head <memory-global>outer <memory-session>inner</memory-session> tail</memory-global> end";
    let result = filter_injected_memory(input);
    assert_eq!(result, "head  end");
}

#[test]
fn filter_memory_mismatched_tags_match() {
    let input = "<memory-foo>content</memory-bar> safe";
    let result = filter_injected_memory(input);
    assert_eq!(result, " safe");
}

#[test]
fn filter_memory_numeric_suffix_matched() {
    let input = "<memory-v2>secret</memory-v2> rest";
    let result = filter_injected_memory(input);
    assert_eq!(result, " rest");
}

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

#[test]
fn filter_memory_code_discussion_false_positive() {
    let input = "The IDE uses <memory-global>...</memory-global> tags for injection.";
    let result = filter_injected_memory(input);
    assert_eq!(result, "The IDE uses  tags for injection.");
}

#[test]
fn filter_memory_markdown_code_block_false_positive() {
    let input = "Example:\n```\n<memory-global>example data</memory-global>\n```\nEnd";
    let result = filter_injected_memory(input);
    assert_eq!(result, "Example:\n```\n\n```\nEnd");
}

#[test]
fn filter_memory_large_content_no_redos() {
    let big_content = "x".repeat(1_000_000);
    let input = format!("<memory-global>{}</memory-global>", big_content);
    let start = std::time::Instant::now();
    let result = filter_injected_memory(&input);
    let elapsed = start.elapsed();
    assert_eq!(result, "");
    assert!(
        elapsed.as_secs() < 2,
        "Parser took {:?} — potential perf issue",
        elapsed
    );
}

#[test]
fn filter_memory_large_content_no_match_no_redos() {
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

#[test]
fn filter_memory_lazy_match_does_not_cross_blocks() {
    let input = "<memory-global>a</memory-global> KEEP THIS <memory-project>b</memory-project>";
    let result = filter_injected_memory(input);
    assert_eq!(result, " KEEP THIS ");
}

#[test]
fn filter_memory_empty_tag() {
    let input = "before <memory-global></memory-global> after";
    let result = filter_injected_memory(input);
    assert_eq!(result, "before  after");
}

#[test]
fn filter_memory_mixed_case_suffix() {
    let input = "<Memory-Global>content</Memory-Global> rest";
    let result = filter_injected_memory(input);
    assert_eq!(result, " rest");
}

#[test]
fn filter_memory_underscore_suffix_matched() {
    let input = "<memory-per_project>data</memory-per_project> rest";
    let result = filter_injected_memory(input);
    assert_eq!(result, " rest");
}

#[test]
fn filter_memory_multiple_unclosed_tags_stripped() {
    let input = "<memory-global>leak1 <memory-project>leak2 <memory-session>leak3";
    let result = filter_injected_memory(input);
    assert_eq!(result, "");
}

#[test]
fn filter_memory_orphaned_close_tag() {
    let input = "before </memory-global> after";
    let result = filter_injected_memory(input);
    assert_eq!(result, "before  after");
}
