use globset::{Glob, GlobSet, GlobSetBuilder};

#[derive(Clone)]
pub struct ProjectFilter {
    /// Original patterns (preserves glob syntax like `[a-z]`)
    raw_matcher: GlobSet,
    /// Normalized patterns (lowercase, hyphens→underscores for ProjectId matching)
    normalized_matcher: GlobSet,
}

impl ProjectFilter {
    pub fn new(raw_patterns: Option<&str>) -> Option<Self> {
        Self::from_env_value(raw_patterns)
    }

    pub fn is_excluded(&self, project: &str) -> bool {
        self.raw_matcher.is_match(project) || self.normalized_matcher.is_match(project)
    }

    fn from_env_value(raw: Option<&str>) -> Option<Self> {
        let value = raw?;
        Self::from_patterns(value.split(',').map(str::trim).filter(|p| !p.is_empty()))
    }

    fn from_patterns<'a, I>(patterns: I) -> Option<Self>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut raw_builder = GlobSetBuilder::new();
        let mut norm_builder = GlobSetBuilder::new();
        let mut added = 0usize;

        for pattern in patterns {
            let expanded = expand_home(pattern);

            if let Ok(glob) = Glob::new(&expanded) {
                raw_builder.add(glob);
            }

            // Normalized pattern — lowercase + hyphens→underscores for ProjectId matching.
            // This may corrupt character classes ([a-z] → [a_z]) but that's fine:
            // the raw matcher already handles those correctly.
            let normalized = expanded.to_lowercase().replace('-', "_");
            if let Ok(glob) = Glob::new(&normalized) {
                norm_builder.add(glob);
                added = added.saturating_add(1);
            }
        }

        if added == 0 {
            return None;
        }

        let raw_matcher = raw_builder.build().ok()?;
        let normalized_matcher = norm_builder.build().ok()?;
        Some(Self {
            raw_matcher,
            normalized_matcher,
        })
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
        // Normalized matcher: my_secret_project matches
        assert!(filter.is_excluded("my_secret_project"));
        // Raw matcher: exact case match
        assert!(filter.is_excluded("My-Secret-Project"));
    }

    #[test]
    fn normalizes_glob_patterns_with_wildcards() {
        let filter = ProjectFilter::from_env_value(Some("My-Secret-*")).expect("filter");
        assert!(filter.is_excluded("my_secret_project"));
        assert!(filter.is_excluded("my_secret_other"));
    }

    #[test]
    fn preserves_glob_character_classes() {
        // [a-z] must NOT become [a_z] — raw matcher preserves original syntax
        let filter = ProjectFilter::from_env_value(Some("[a-z]*_project")).expect("filter");
        assert!(filter.is_excluded("my_project"));
        assert!(filter.is_excluded("x_project"));
        assert!(!filter.is_excluded("1_project"));
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
