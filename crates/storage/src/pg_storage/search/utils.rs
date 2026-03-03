pub(crate) fn build_tsquery(query: &str) -> Option<String> {
    let result = query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter_map(|w| {
            if !w.chars().any(char::is_alphanumeric) {
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

pub(crate) fn build_or_tsquery(query: &str, max_terms: usize) -> Option<String> {
    let mut words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter_map(|w| {
            // Must contain at least one alphanumeric
            // (rejects "---", "___" which cause tsquery syntax errors)
            if !w.chars().any(char::is_alphanumeric) {
                None
            } else {
                Some(w.to_string())
            }
        })
        .collect();

    words.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
    words.truncate(max_terms);

    let result = words.into_iter().map(|w| format!("{}:*", w)).collect::<Vec<_>>().join(" | ");

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}
