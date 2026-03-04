fn tokenize_tsquery(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter_map(|w| {
            if w.chars().any(char::is_alphanumeric) {
                Some(w.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn build_joined_tsquery(words: Vec<String>, operator: &str) -> Option<String> {
    if words.is_empty() {
        return None;
    }
    Some(
        words
            .into_iter()
            .map(|w| format!("{}:*", w))
            .collect::<Vec<_>>()
            .join(operator),
    )
}

pub(crate) fn build_tsquery(query: &str) -> Option<String> {
    let mut words = tokenize_tsquery(query);
    words.truncate(100); // Prevent DoS from massive input
    build_joined_tsquery(words, " & ")
}

pub(crate) fn build_or_tsquery(query: &str, max_terms: usize) -> Option<String> {
    let mut words = tokenize_tsquery(query);
    // Must contain at least one alphanumeric
    // (rejects "---", "___" which cause tsquery syntax errors)
    words.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
    words.truncate(max_terms);
    build_joined_tsquery(words, " | ")
}
