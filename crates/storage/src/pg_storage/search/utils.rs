pub(crate) fn build_tsquery(query: &str) -> Option<String> {
    let result = query
        .split_whitespace()
        .filter_map(|w| {
            // Strip tsquery operators and special characters, keep only alphanumeric
            let sanitized: String =
                w.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_').collect();
            if sanitized.is_empty() || !sanitized.chars().any(char::is_alphanumeric) {
                None
            } else {
                Some(format!("{}:*", sanitized))
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
        .split_whitespace()
        .filter_map(|w| {
            let sanitized: String =
                w.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_').collect();
            // Must have at least 2 chars AND contain at least one alphanumeric
            // (rejects "---", "___" which cause tsquery syntax errors)
            if sanitized.chars().count() < 2 || !sanitized.chars().any(char::is_alphanumeric) {
                None
            } else {
                Some(sanitized)
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
