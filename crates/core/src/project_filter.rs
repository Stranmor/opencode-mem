use globset::{Glob, GlobSet, GlobSetBuilder};
use std::sync::OnceLock;

const EXCLUDED_PROJECTS_ENV: &str = "OPENCODE_MEM_EXCLUDED_PROJECTS";
static PROJECT_FILTER: OnceLock<Option<ProjectFilter>> = OnceLock::new();

pub struct ProjectFilter {
    matcher: GlobSet,
}

impl ProjectFilter {
    pub fn from_env() -> Option<Self> {
        Self::from_env_value(std::env::var(EXCLUDED_PROJECTS_ENV).ok().as_deref())
    }

    pub fn global() -> Option<&'static Self> {
        PROJECT_FILTER.get_or_init(Self::from_env).as_ref()
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
            if let Ok(glob) = Glob::new(&expanded) {
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
        return dirs::home_dir()
            .map_or_else(|| pattern.to_owned(), |p| format!("{}/{}", p.display(), rest));
    }
    pattern.to_owned()
}

#[cfg(test)]
mod tests {
    use super::{expand_home, ProjectFilter};
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

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
    fn returns_none_for_empty_env_value() {
        assert!(ProjectFilter::from_env_value(Some("   ,  ")).is_none());
    }

    #[test]
    fn returns_none_for_missing_env_value() {
        assert!(ProjectFilter::from_env_value(None).is_none());
    }

    #[test]
    fn returns_none_for_empty_env_var() {
        let _guard = env_lock().lock().expect("lock");
        std::env::set_var("OPENCODE_MEM_EXCLUDED_PROJECTS", " , ");
        let filter = ProjectFilter::from_env();
        std::env::remove_var("OPENCODE_MEM_EXCLUDED_PROJECTS");
        assert!(filter.is_none());
    }

    #[test]
    fn returns_none_for_missing_env_var() {
        let _guard = env_lock().lock().expect("lock");
        std::env::remove_var("OPENCODE_MEM_EXCLUDED_PROJECTS");
        assert!(ProjectFilter::from_env().is_none());
    }
}
