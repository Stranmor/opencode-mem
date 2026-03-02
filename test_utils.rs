fn build_tsquery(query: &str) -> Option<String> {
    let result = query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter_map(|w| {
            if w.chars().count() < 2 || !w.chars().any(char::is_alphanumeric) {
                None
            } else {
                Some(format!("{}:*", w))
            }
        })
        .collect::<Vec<_>>()
        .join(" & ");
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

fn build_or_tsquery(query: &str, max_terms: usize) -> Option<String> {
    let mut words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter_map(|w| {
            // Must have at least 2 chars AND contain at least one alphanumeric
            // (rejects "---", "___" which cause tsquery syntax errors)
            if w.chars().count() < 2 || !w.chars().any(char::is_alphanumeric) {
                None
            } else {
                Some(w.to_string())
            }
        })
        .collect();
    words.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
    words.truncate(max_terms);
    let result = words
        .into_iter()
        .map(|w| format!("{}:*", w))
        .collect::<Vec<_>>()
        .join(" | ");
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

fn main() {
    assert_eq!(build_tsquery("src/utils.rs"), Some("src:* & utils:* & rs:*".to_string()));
    assert_eq!(build_tsquery("hello-world"), Some("hello:* & world:*".to_string()));
    assert_eq!(build_tsquery("user.name@email.com"), Some("user:* & name:* & email:* & com:*".to_string()));
    assert_eq!(build_tsquery("a/b/c"), None);
    assert_eq!(build_tsquery("a b c d e"), None);
    assert_eq!(build_tsquery("ab c d ef"), Some("ab:* & ef:*".to_string()));
    assert_eq!(build_tsquery("___"), None);
    assert_eq!(build_tsquery(""), None);

    assert_eq!(build_or_tsquery("src/utils.rs", 10), Some("utils:* & src:* & rs:*".replace(" & ", " | ")));
}
