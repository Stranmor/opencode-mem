use globset::{Glob, GlobSet, GlobSetBuilder};

#[derive(Clone)]
pub struct ProjectFilter {
    matcher: GlobSet,
}

impl ProjectFilter {
    pub fn new(raw_patterns: Option<&str>) -> Option<Self> {
        Self::from_env_value(raw_patterns)
    }

    pub fn is_excluded(&self, project: &str) -> bool {
        self.matcher.is_match(project)
    }

    fn from_env_value(raw: Option<&str>) -> Option<Self> {
        let value = raw?;
        Self::from_patterns(value.split(',').map(str::trim).filter(|p| !p.is_empty()))
    }

    fn from_patterns<'a, I>(patterns: I) -> Option<Self>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut builder = GlobSetBuilder::new();
        let mut added = 0usize;

        for pattern in patterns {
            let expanded = expand_home(pattern);
            // Normalize pattern the same way ProjectId::normalize() does
            // (lowercase, hyphens → underscores) so that a pattern like
            // "My-Secret-Project" matches the normalized input "my_secret_project".
            let normalized = expanded.to_lowercase().replace('-', "_");
            if let Ok(glob) = Glob::new(&normalized) {
                builder.add(glob);
                added = added.saturating_add(1);
            }
        }

        if added == 0 {
            return None;
        }

        let matcher = builder.build().ok()?;
        Some(Self { matcher })
    }
}

fn expand_home(pattern: &str) -> String {
    if pattern == "~" {
        return dirs::home_dir().map_or_else(|| pattern.to_owned(), |p| p.display().to_string());
    }
    if let Some(rest) = pattern.strip_prefix("~/") {
        return dirs::home_dir().map_or_else(
            || pattern.to_owned(),
            |p| format!("{}/{}", p.display(), rest),
        );
    }
    pattern.to_owned()
}

#[cfg(test)]
mod tests {
    use super::{ProjectFilter, expand_home};

    #[test]
    fn matches_basic_glob_pattern() {
        let filter = ProjectFilter::from_env_value(Some("/tmp/*")).expect("filter");
        assert!(filter.is_excluded("/tmp/foo"));
        assert!(!filter.is_excluded("/var/tmp/foo"));
    }

    #[test]
    fn matches_recursive_glob_pattern() {
        let filter = ProjectFilter::from_env_value(Some("/home/user/**")).expect("filter");
        assert!(filter.is_excluded("/home/user/project/src"));
        assert!(!filter.is_excluded("/home/other/project/src"));
    }

    #[test]
    fn expands_home_prefix() {
        let expanded = expand_home("~/kunden/**");
        let expected_prefix = dirs::home_dir().expect("home dir").display().to_string();
        assert!(expanded.starts_with(&expected_prefix));
        assert!(expanded.ends_with("kunden/**"));
    }

    #[test]
    fn normalizes_patterns_to_match_project_id() {
        let filter = ProjectFilter::from_env_value(Some("My-Secret-Project")).expect("filter");
        // ProjectId::new("My-Secret-Project").to_string() == "my_secret_project"
        assert!(filter.is_excluded("my_secret_project"));
        assert!(!filter.is_excluded("My-Secret-Project"));
    }

    #[test]
    fn normalizes_glob_patterns_with_wildcards() {
        let filter = ProjectFilter::from_env_value(Some("My-Secret-*")).expect("filter");
        assert!(filter.is_excluded("my_secret_project"));
        assert!(filter.is_excluded("my_secret_other"));
    }

    #[test]
    fn returns_none_for_empty_env_value() {
        assert!(ProjectFilter::from_env_value(Some("   ,  ")).is_none());
    }

    #[test]
    fn returns_none_for_missing_env_value() {
        assert!(ProjectFilter::from_env_value(None).is_none());
    }
}
