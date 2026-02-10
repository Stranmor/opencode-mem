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
            contains: BASE_CONTAINS.iter().map(|v| (*v).into()).collect(),
            prefixes: BASE_PREFIXES.iter().map(|v| (*v).into()).collect(),
            exact: BASE_EXACT.iter().map(|v| (*v).into()).collect(),
        };
        if let Ok(p) = env::var("OPENCODE_MEM_FILTER_PATTERNS") {
            let parsed = Self::from_pattern_str(&p);
            filter.contains.extend(parsed.contains);
            filter.prefixes.extend(parsed.prefixes);
            filter.exact.extend(parsed.exact);
        }
        for v in [&mut filter.contains, &mut filter.prefixes, &mut filter.exact] {
            v.sort_unstable();
            v.dedup();
        }
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
                    if let Some(v) = Some(chars.as_str().trim()).filter(|s| !s.is_empty()) {
                        filter.prefixes.push((*v).into());
                    }
                },
                Some('=') => {
                    if let Some(v) = Some(chars.as_str().trim()).filter(|s| !s.is_empty()) {
                        filter.exact.push((*v).into());
                    }
                },
                _ => filter.contains.push(token.into()),
            }
        }
        for v in [&mut filter.contains, &mut filter.prefixes, &mut filter.exact] {
            v.sort_unstable();
            v.dedup();
        }
        filter
    }

    fn matches(&self, t: &str) -> bool {
        self.exact.iter().any(|v| t == v.as_ref())
            || self.prefixes.iter().any(|v| t.starts_with(v.as_ref()))
            || self.contains.iter().any(|v| t.contains(v.as_ref()))
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
    fn as_strs(v: &[Box<str>]) -> Vec<&str> {
        v.iter().map(|x| x.as_ref()).collect()
    }

    #[test]
    fn test_parsing() {
        let f = LowValueFilter::from_pattern_str("a,^b,=c, ,a,^b,=c");
        assert_eq!(as_strs(&f.contains), vec!["a"]);
        assert_eq!(as_strs(&f.prefixes), vec!["b"]);
        assert_eq!(as_strs(&f.exact), vec!["c"]);
    }

    #[test]
    fn test_filtering() {
        let low = [
            "File edit applied successfully",
            "rustfmt nightly formatting",
            "Agent behavioral protocol update",
            "Updated TODO list",
            "Search results for auth",
            "keyword frequency analysis",
        ];
        for title in low {
            assert!(is_low_value_observation(title), "Should be low value: {}", title);
        }

        let high = [
            "Database migration v10 added session_summaries",
            "Fixed race condition",
            "Fixing critical bug",
            "Refined scoring logic",
            "",
        ];
        for title in high {
            assert!(!is_low_value_observation(title), "Should be high value: {}", title);
        }
    }

    #[test]
    fn test_case_and_partial() {
        assert!(is_low_value_observation("FILE EDIT APPLIED SUCCESSFULLY"));
        assert!(is_low_value_observation("There is no significant change"));
    }
}
