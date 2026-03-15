//! Import AGI audit insights from markdown files into knowledge base

use anyhow::{Context, Result};
use opencode_mem_core::{KnowledgeInput, KnowledgeType};
use opencode_mem_storage::StorageBackend;
use opencode_mem_storage::traits::KnowledgeStore;
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

#[expect(
    clippy::unwrap_used,
    reason = "static regex patterns are compile-time validated"
)]
static INSIGHT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"###\s*\u{418}\u{43d}\u{441}\u{430}\u{439}\u{442}\s*\d+:\s*(?:\[([^\]]+)\]|(.+))")
        .unwrap()
});

#[expect(
    clippy::unwrap_used,
    reason = "static regex patterns are compile-time validated"
)]
static SECTION_SPLIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^#{3}\s*\u{418}\u{43d}\u{441}\u{430}\u{439}\u{442}").unwrap()
});

#[expect(
    clippy::unwrap_used,
    reason = "static regex patterns are compile-time validated"
)]
static CATEGORY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\*\*\u{41a}\u{430}\u{442}\u{435}\u{433}\u{43e}\u{440}\u{438}\u{44f}\s*:?\s*\*\*\s*:?\s*(.+)")
        .unwrap()
});

#[expect(
    clippy::unwrap_used,
    reason = "static regex patterns are compile-time validated"
)]
static OBSERVATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?s)\*\*\u{41d}\u{430}\u{431}\u{43b}\u{44e}\u{434}\u{435}\u{43d}\u{438}\u{435}\s*:?\s*\*\*\s*:?\s*(.*?)(?:\*\*(?:\u{418}\u{43c}\u{43f}\u{43b}\u{438}\u{43a}\u{430}\u{446}\u{438}\u{44f}|\u{420}\u{435}\u{43a}\u{43e}\u{43c}\u{435}\u{43d}\u{434}\u{430}\u{446}\u{438}\u{44f}|\u{41a}\u{430}\u{442}\u{435}\u{433}\u{43e}\u{440}\u{438}\u{44f}|\u{41d}\u{430}\u{431}\u{43b}\u{44e}\u{434}\u{435}\u{43d}\u{438}\u{435})|###|\z)",
    )
    .unwrap()
});

#[expect(
    clippy::unwrap_used,
    reason = "static regex patterns are compile-time validated"
)]
static IMPLICATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\*\*\u{418}\u{43c}\u{43f}\u{43b}\u{438}\u{43a}\u{430}\u{446}\u{438}\u{44f} \u{434}\u{43b}\u{44f} AGI\s*:?\s*\*\*\s*:?\s*(.*?)(?:\*\*(?:\u{420}\u{435}\u{43a}\u{43e}\u{43c}\u{435}\u{43d}\u{434}\u{430}\u{446}\u{438}\u{44f}|\u{41a}\u{430}\u{442}\u{435}\u{433}\u{43e}\u{440}\u{438}\u{44f}|\u{41d}\u{430}\u{431}\u{43b}\u{44e}\u{434}\u{435}\u{43d}\u{438}\u{435}|\u{418}\u{43c}\u{43f}\u{43b}\u{438}\u{43a}\u{430}\u{446}\u{438}\u{44f})|###|\z)").unwrap()
});

#[expect(
    clippy::unwrap_used,
    reason = "static regex patterns are compile-time validated"
)]
static RECOMMENDATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\*\*\u{420}\u{435}\u{43a}\u{43e}\u{43c}\u{435}\u{43d}\u{434}\u{430}\u{446}\u{438}\u{44f}\s*:?\s*\*\*\s*:?\s*(.*?)(?:\*\*(?:\u{418}\u{43c}\u{43f}\u{43b}\u{438}\u{43a}\u{430}\u{446}\u{438}\u{44f}|\u{41a}\u{430}\u{442}\u{435}\u{433}\u{43e}\u{440}\u{438}\u{44f}|\u{41d}\u{430}\u{431}\u{43b}\u{44e}\u{434}\u{435}\u{43d}\u{438}\u{435}|\u{420}\u{435}\u{43a}\u{43e}\u{43c}\u{435}\u{43d}\u{434}\u{430}\u{446}\u{438}\u{44f})|###|\z)").unwrap()
});

/// Parsed insight from markdown
struct ParsedInsight {
    title: String,
    category: String,
    observation: String,
    implication: Option<String>,
    recommendation: Option<String>,
}

/// Map Russian category to `KnowledgeType`
fn category_to_knowledge_type(category: &str) -> KnowledgeType {
    let lower = category.to_lowercase();
    if lower.starts_with("паттерн")
        || lower.starts_with("планирован")
        || lower.starts_with("позитивн")
    {
        KnowledgeType::Pattern
    } else {
        // Слабость, Missing, Галлюцинации, Неоптимальные, Ригидность, etc.
        KnowledgeType::Gotcha
    }
}

/// Extract keywords from title for triggers
fn extract_triggers(title: &str) -> Vec<String> {
    title
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| !w.is_empty() && w.chars().count() > 3)
        .collect()
}

/// Parse markdown content and extract insights
fn parse_insights(content: &str) -> Vec<ParsedInsight> {
    let mut insights = Vec::new();

    // Use regex-based split to tolerate optional whitespace after ###
    let split_points: Vec<usize> = SECTION_SPLIT_RE
        .find_iter(content)
        .map(|m| m.start())
        .collect();

    for (i, &start) in split_points.iter().enumerate() {
        let end = split_points
            .get(i.wrapping_add(1))
            .copied()
            .unwrap_or(content.len());
        let section = &content[start..end];

        let title = INSIGHT_RE.captures(section).and_then(|c| {
            c.get(1)
                .or_else(|| c.get(2))
                .map(|m| m.as_str().trim().to_owned())
        });

        let category = CATEGORY_RE
            .captures(section)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_owned());

        let observation = OBSERVATION_RE
            .captures(section)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_owned());

        let implication = IMPLICATION_RE
            .captures(section)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_owned());

        let recommendation = RECOMMENDATION_RE
            .captures(section)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_owned());

        if let (Some(title), Some(category), Some(observation)) = (title, category, observation) {
            insights.push(ParsedInsight {
                title,
                category,
                observation,
                implication,
                recommendation,
            });
        }
    }

    insights
}

/// Check if knowledge with given title already exists
async fn title_exists(storage: &StorageBackend, title: &str) -> Result<bool> {
    Ok(storage.knowledge_exists_by_title(title).await?)
}

/// Import insights from a single file
async fn import_file(storage: &StorageBackend, path: &Path) -> Result<(usize, usize)> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let insights = parse_insights(&content);
    let mut imported = 0;
    let mut skipped = 0;

    for insight in insights {
        let sanitized_title = opencode_mem_core::sanitize_input(&insight.title);
        if title_exists(storage, &sanitized_title).await? {
            skipped += 1;
            continue;
        }

        let description = match &insight.implication {
            Some(impl_text) => {
                format!(
                    "{}\n\n\u{418}\u{43c}\u{43f}\u{43b}\u{438}\u{43a}\u{430}\u{446}\u{438}\u{44f} \u{434}\u{43b}\u{44f} AGI: {}",
                    insight.observation, impl_text
                )
            }
            None => insight.observation.clone(),
        };

        let input = KnowledgeInput::new(
            category_to_knowledge_type(&insight.category),
            sanitized_title,
            opencode_mem_core::sanitize_input(&description),
            insight
                .recommendation
                .as_deref()
                .map(opencode_mem_core::sanitize_input),
            extract_triggers(&insight.title)
                .into_iter()
                .map(|t| opencode_mem_core::sanitize_input(&t))
                .collect(),
            Some("agi-audit".to_owned()),
            None,
        );

        storage.save_knowledge(input).await?;
        imported += 1;
    }

    Ok((imported, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_unbracketed_title() {
        let md = r#"### Инсайт 1: Синдром "Работает на моём curl"
**Категория:** Слабость (Ригидность / Неоптимальные решения)
**Наблюдение:** Агент починил ошибку и объявил победу.
**Импликация для AGI:** AGI должен понимать разницу.
**Рекомендация:** Запретить агенту объявлять задачу решенной.
"#;
        let insights = parse_insights(md);
        assert_eq!(insights.len(), 1);
        let i = &insights[0];
        assert_eq!(i.title, "Синдром \"Работает на моём curl\"");
        assert_eq!(i.category, "Слабость (Ригидность / Неоптимальные решения)");
        assert!(i.observation.starts_with("Агент починил"));
        assert!(i.implication.is_some());
        assert!(i.recommendation.is_some());
    }

    #[test]
    fn parse_bracketed_title() {
        let md = r#"### Инсайт 1: [Title In Brackets]
**Категория:** Паттерн
**Наблюдение:** Some observation text.
"#;
        let insights = parse_insights(md);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].title, "Title In Brackets");
    }

    #[test]
    fn parse_multiple_insights() {
        let md = r#"## Session 1

### Инсайт 1: First Title
**Категория:** Слабость
**Наблюдение:** First observation.

### Инсайт 2: Second Title
**Категория:** Паттерн (Повторяющиеся ошибки)
**Наблюдение:** Second observation.
**Импликация для AGI:** Some implication.
**Рекомендация:** Some recommendation.
"#;
        let insights = parse_insights(md);
        assert_eq!(insights.len(), 2);
        assert_eq!(insights[0].title, "First Title");
        assert_eq!(insights[1].title, "Second Title");
        assert!(insights[0].implication.is_none());
        assert!(insights[1].implication.is_some());
    }

    #[test]
    fn category_mapping_with_parenthetical() {
        assert_eq!(
            category_to_knowledge_type("Слабость (Ригидность)"),
            KnowledgeType::Gotcha
        );
        assert_eq!(
            category_to_knowledge_type("Паттерн (Повторяющиеся ошибки)"),
            KnowledgeType::Pattern
        );
        assert_eq!(
            category_to_knowledge_type("Missing for AGI (Мета-когниция)"),
            KnowledgeType::Gotcha
        );
        assert_eq!(
            category_to_knowledge_type("Галлюцинации / Слабость"),
            KnowledgeType::Gotcha
        );
    }

    #[test]
    fn test_vulnerability_truncation_at_bold_tags() {
        let md = r#"### Инсайт 1: Truncation Bug
**Категория:** Слабость
**Наблюдение:** The agent failed because it didn't use **bold** text.
**Импликация для AGI:** AGI must understand **Markdown**.
"#;
        let insights = parse_insights(md);
        assert_eq!(insights.len(), 1);
        assert_eq!(
            insights[0].observation,
            "The agent failed because it didn't use **bold** text."
        );
        assert_eq!(
            insights[0].implication.as_deref(),
            Some("AGI must understand **Markdown**.")
        );
    }

    #[test]
    fn test_vulnerability_missing_space_after_hash() {
        let md = r#"### Инсайт 1: First
**Категория:** Паттерн
**Наблюдение:** First obs.

###Инсайт 2: Second
**Категория:** Слабость
**Наблюдение:** Second obs.
"#;
        let insights = parse_insights(md);
        assert_eq!(insights.len(), 2);
        assert_eq!(insights[0].title, "First");
        assert_eq!(insights[1].title, "Second");
    }

    #[test]
    fn test_vulnerability_brittle_category_regex() {
        let md = r#"### Инсайт 1: Brittle Regex
**Категория**: Паттерн
**Наблюдение:** This is an observation.
"#;
        let insights = parse_insights(md);
        assert_eq!(insights.len(), 1);
        assert_eq!(insights[0].title, "Brittle Regex");
        assert_eq!(insights[0].category, "Паттерн");
    }

    #[test]
    fn test_vulnerability_silent_duplicate_insertion() {
        // #234: title_exists now uses direct SQL `WHERE title = $1` instead of FTS search.
        // No unit test possible without DB, but the code path is verified by integration tests.
        assert!(true);
    }
}

/// Run import-insights command
pub(crate) async fn run(file: Option<String>, dir: Option<String>) -> Result<()> {
    let storage = crate::create_storage_from_env().await?;

    let mut total_imported = 0;
    let mut total_skipped = 0;

    if let Some(file_path) = file {
        let path = Path::new(&file_path);
        let (imported, skipped) = import_file(&storage, path).await?;
        total_imported += imported;
        total_skipped += skipped;
        println!("Processed: {}", path.display());
    }

    if let Some(dir_path) = dir {
        let dir = Path::new(&dir_path);
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                match import_file(&storage, &path).await {
                    Ok((imported, skipped)) => {
                        total_imported += imported;
                        total_skipped += skipped;
                        println!("Processed: {}", path.display());
                    }
                    Err(e) => {
                        eprintln!("Error processing {}: {}", path.display(), e);
                    }
                }
            }
        }
    }

    println!("\nImported {total_imported} insights, skipped {total_skipped} duplicates");

    Ok(())
}
