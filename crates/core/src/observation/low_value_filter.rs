use std::env;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

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
    "marked as completed",
    "memory classification",
    "memory storage classification",
    "no significant",
    "noise level classification",
    "standardized ",
    "successful file edit",
    "task completion signal",
    "term frequency",
    "test execution",
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
    "connectivity check",
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
    "discovered ",
    "documented ",
    "draft ",
    "established ",
    "evolution ",
    "enhancement plan ",
    "examined ",
    "explored ",
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
    "planning ",
    "progress ",
    "prohibition ",
    "pulled ",
    "pushed ",
    "ran ",
    "read ",
    "recent ",
    "refactored ",
    "refactoring ",
    "refactor plan",
    "removed ",
    "renamed ",
    "resolved ",
    "retrieved ",
    "roadmap for ",
    "roadmap: ",
    "robust ",
    "routine ",
    "scanned ",
    "shared ",
    "started ",
    "status ",
    "stopped ",
    "strategy for ",
    "structure ",
    "successful ",
    "summary of ",
    "syntax error",
    "task list ",
    "task progress",
    "task status",
    "tracking ",
    "transition ",
    "updated ",
    "verification ",
    "verified ",
    "wip: ",
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
    let t = normalize_title(title);
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

fn deconfuse(c: char) -> char {
    match c {
        'а' => 'a',
        'е' => 'e',
        'о' => 'o',
        'р' => 'p',
        'с' => 'c',
        'у' => 'y',
        'х' => 'x',
        'і' => 'i',
        _ => c,
    }
}

fn normalize_title(title: &str) -> String {
    title
        .nfkd()
        .map(deconfuse)
        .map(|c| {
            if c.is_control()
                || c == '\u{200B}'
                || c == '\u{200C}'
                || c == '\u{200D}'
                || c == '\u{FEFF}'
                || matches!(c, '\u{FE00}'..='\u{FE0F}')
            {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .to_lowercase()
}

#[cfg(test)]
mod tests;
