use super::super::*;

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
