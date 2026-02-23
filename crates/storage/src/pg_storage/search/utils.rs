pub(crate) fn build_tsquery(query: &str) -> Option<String> {
    let result = query
        .split_whitespace()
        .filter_map(|w| {
            // Strip tsquery operators and special characters, keep only alphanumeric
            let sanitized: String =
                w.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_').collect();
            if sanitized.is_empty() {
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
