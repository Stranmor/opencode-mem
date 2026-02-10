use std::env;
use std::sync::LazyLock;

const BASE_CONTAINS: &[&str] = &[
    "code edits",
    "code quality",
    "code review",
    "compilation ",
    "component frequency",
    "documentation index",
    "edit applied",
    "file edit applied successfully",
    "keyword frequency",
    "knowledge index",
    "memory classification",
    "memory storage classification",
    "no significant",
    "noise level classification",
    "standardized ",
    "successful file edit",
    "task completion signal",
    "term frequency",
    "tool call observed",
    "tool execution",
];

const BASE_PREFIXES: &[&str] = &[
    "active ",
    "added ",
    "agentic ",
    "analyzed ",
    "application ",
    "applied ",
    "architectural ",
    "audit of ",
    "backend ",
    "broken ",
    "build ",
    "centralizing ",
    "checked ",
    "cleanup ",
    "closed ",
    "codebase ",
    "committed ",
    "completed ",
    "comprehensive ",
    "confirmed ",
    "created ",
    "definition ",
    "delegated ",
    "deleted ",
    "deployment ",
    "detected ",
    "development ",
    "discovery of ",
    "documented ",
    "draft ",
    "established ",
    "evolution ",
    "examined ",
    "executed ",
    "extracted ",
    "fetched ",
    "finished ",
    "fixing ",
    "found ",
    "frequency ",
    "frontend ",
    "generated ",
    "identification ",
    "identified ",
    "implemented ",
    "implementing ",
    "improved ",
    "index of ",
    "initiated ",
    "inspected ",
    "integrated ",
    "inventory of ",
    "launched ",
    "linter ",
    "linting ",
    "list of ",
    "located ",
    "location ",
    "mandatory ",
    "manual ",
    "map of ",
    "mapping of ",
    "marked ",
    "merged ",
    "migrated ",
    "modified ",
    "module ",
    "moved ",
    "multiple ",
    "new ",
    "observed ",
    "opened ",
    "overview of ",
    "pending ",
    "planned ",
    "progress ",
    "prohibition ",
    "pulled ",
    "pushed ",
    "ran ",
    "read ",
    "recent ",
    "refactored ",
    "refactoring ",
    "removed ",
    "renamed ",
    "resolved ",
    "retrieved ",
    "roadmap for ",
    "roadmap: ",
    "robust ",
    "scanned ",
    "shared ",
    "started ",
    "status ",
    "stopped ",
    "structure ",
    "summary of ",
    "tracking ",
    "transition ",
    "updated agents.md",
    "updated plan",
    "updated task status",
    "updated todo",
    "verification ",
    "verified ",
    "workflow ",
    "wrote ",
];

const BASE_EXACT: &[&str] = &["task completion"];

struct LowValueFilter {
    contains: Vec<Box<str>>,
    prefixes: Vec<Box<str>>,
    exact: Vec<Box<str>>,
}

static LOW_VALUE_FILTER: LazyLock<LowValueFilter> = LazyLock::new(LowValueFilter::from_env);

impl LowValueFilter {
    fn from_env() -> Self {
        let mut filter = Self {
            contains: BASE_CONTAINS.iter().map(|value| (*value).into()).collect(),
            prefixes: BASE_PREFIXES.iter().map(|value| (*value).into()).collect(),
            exact: BASE_EXACT.iter().map(|value| (*value).into()).collect(),
        };

        if let Ok(patterns) = env::var("OPENCODE_MEM_FILTER_PATTERNS") {
            let parsed = Self::from_pattern_str(&patterns);
            filter.contains.extend(parsed.contains);
            filter.prefixes.extend(parsed.prefixes);
            filter.exact.extend(parsed.exact);
        }

        filter.contains.sort_unstable();
        filter.contains.dedup();
        filter.prefixes.sort_unstable();
        filter.prefixes.dedup();
        filter.exact.sort_unstable();
        filter.exact.dedup();

        filter
    }

    fn from_pattern_str(patterns: &str) -> Self {
        let mut filter = Self { contains: Vec::new(), prefixes: Vec::new(), exact: Vec::new() };

        for raw in patterns.split(',') {
            let token = raw.trim();
            if token.is_empty() {
                continue;
            }
            let token = token.to_lowercase();
            let mut chars = token.chars();
            match chars.next() {
                Some('^') => {
                    let value = chars.as_str().trim();
                    if !value.is_empty() {
                        filter.prefixes.push(value.into());
                    }
                },
                Some('=') => {
                    let value = chars.as_str().trim();
                    if !value.is_empty() {
                        filter.exact.push(value.into());
                    }
                },
                _ => {
                    filter.contains.push(token.into());
                },
            }
        }

        filter.contains.sort_unstable();
        filter.contains.dedup();
        filter.prefixes.sort_unstable();
        filter.prefixes.dedup();
        filter.exact.sort_unstable();
        filter.exact.dedup();

        filter
    }

    fn matches(&self, title_lower: &str) -> bool {
        for value in &self.exact {
            if title_lower == value.as_ref() {
                return true;
            }
        }

        for value in &self.prefixes {
            if title_lower.starts_with(value.as_ref()) {
                return true;
            }
        }

        for value in &self.contains {
            if title_lower.contains(value.as_ref()) {
                return true;
            }
        }

        false
    }
}

pub fn is_low_value_observation(title: &str) -> bool {
    let t = title.to_lowercase();

    if t.contains("rustfmt") && t.contains("nightly") {
        return true;
    }

    if (t.contains("comment") || t.contains("docstring")) && t.contains("hook") {
        return true;
    }

    if t.starts_with("refined ") && !t.contains("logic") && !t.contains("formula") {
        return true;
    }

    if t.starts_with("search ")
        && (t.contains("results") || t.contains("failed") || t.contains("yielded"))
    {
        return true;
    }

    if t.starts_with("agent ")
        && (t.contains("rules")
            || t.contains("protocol")
            || t.contains("guidelines")
            || t.contains("doctrine")
            || t.contains("principles")
            || t.contains("behavioral")
            || t.contains("operational")
            || t.contains("workflow")
            || t.contains("persona"))
    {
        return true;
    }

    LOW_VALUE_FILTER.matches(&t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn as_strs(values: &[Box<str>]) -> Vec<&str> {
        values.iter().map(|value| value.as_ref()).collect()
    }

    #[test]
    fn parse_contains_patterns() {
        let filter = LowValueFilter::from_pattern_str("alpha,beta");
        assert_eq!(as_strs(&filter.contains), vec!["alpha", "beta"]);
        assert!(filter.prefixes.is_empty());
        assert!(filter.exact.is_empty());
    }

    #[test]
    fn parse_prefix_patterns() {
        let filter = LowValueFilter::from_pattern_str("^alpha,^beta");
        assert_eq!(as_strs(&filter.prefixes), vec!["alpha", "beta"]);
        assert!(filter.contains.is_empty());
        assert!(filter.exact.is_empty());
    }

    #[test]
    fn parse_exact_patterns() {
        let filter = LowValueFilter::from_pattern_str("=alpha,=beta");
        assert_eq!(as_strs(&filter.exact), vec!["alpha", "beta"]);
        assert!(filter.contains.is_empty());
        assert!(filter.prefixes.is_empty());
    }

    #[test]
    fn parse_ignores_empty_tokens() {
        let filter = LowValueFilter::from_pattern_str(" , alpha, ,^beta,=gamma, ");
        assert_eq!(as_strs(&filter.contains), vec!["alpha"]);
        assert_eq!(as_strs(&filter.prefixes), vec!["beta"]);
        assert_eq!(as_strs(&filter.exact), vec!["gamma"]);
    }

    #[test]
    fn parse_deduplicates_patterns() {
        let filter = LowValueFilter::from_pattern_str("alpha,alpha,^beta,^beta,=gamma,=gamma");
        assert_eq!(as_strs(&filter.contains), vec!["alpha"]);
        assert_eq!(as_strs(&filter.prefixes), vec!["beta"]);
        assert_eq!(as_strs(&filter.exact), vec!["gamma"]);
    }

    #[test]
    fn low_value_edit_applied() {
        assert!(is_low_value_observation("File edit applied successfully"));
        assert!(is_low_value_observation("Edit Applied to config.rs"));
    }

    #[test]
    fn low_value_rustfmt_nightly() {
        assert!(is_low_value_observation("rustfmt nightly formatting"));
    }

    #[test]
    fn low_value_agent_rules() {
        assert!(is_low_value_observation("Agent behavioral protocol update"));
    }

    #[test]
    fn low_value_todo_updates() {
        assert!(is_low_value_observation("Updated TODO list"));
        assert!(is_low_value_observation("updated plan for deployment"));
    }

    #[test]
    fn high_value_passes_filter() {
        assert!(!is_low_value_observation("Database migration v10 added session_summaries"));
        assert!(!is_low_value_observation("Fixed race condition in queue processor"));
    }

    #[test]
    fn low_value_empty_string_passes() {
        assert!(!is_low_value_observation(""));
    }

    #[test]
    fn low_value_case_insensitive_uppercase() {
        assert!(is_low_value_observation("FILE EDIT APPLIED SUCCESSFULLY"));
        assert!(is_low_value_observation("RUSTFMT NIGHTLY formatting"));
        assert!(is_low_value_observation("UPDATED TODO list"));
    }

    #[test]
    fn low_value_partial_match_no_significant() {
        assert!(is_low_value_observation("There is no significant change in this update"));
    }

    #[test]
    fn low_value_new_patterns() {
        assert!(is_low_value_observation("Located auth_middleware definition"));
        assert!(is_low_value_observation("Index of project components"));
        assert!(is_low_value_observation("Completed extract_post_body implementation"));
        assert!(is_low_value_observation("Recent SMM Evolution"));
        assert!(is_low_value_observation("Refined scoring calibration"));
        assert!(!is_low_value_observation("Refined scoring logic for v6"));
        assert!(is_low_value_observation("Search results for auth"));
        assert!(is_low_value_observation("keyword frequency analysis"));
    }
}
