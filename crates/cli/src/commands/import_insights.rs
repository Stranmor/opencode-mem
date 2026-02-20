//! Import AGI audit insights from markdown files into knowledge base

use anyhow::{Context, Result};
use opencode_mem_core::{KnowledgeInput, KnowledgeType};
use opencode_mem_storage::traits::KnowledgeStore;
use opencode_mem_storage::StorageBackend;
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

#[expect(clippy::unwrap_used, reason = "static regex patterns are compile-time validated")]
static INSIGHT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"###\s*\u{418}\u{43d}\u{441}\u{430}\u{439}\u{442}\s*\d+:\s*\[([^\]]+)\]").unwrap()
});

#[expect(clippy::unwrap_used, reason = "static regex patterns are compile-time validated")]
static CATEGORY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\*\*\u{41a}\u{430}\u{442}\u{435}\u{433}\u{43e}\u{440}\u{438}\u{44f}:\*\*\s*(.+)")
        .unwrap()
});

#[expect(clippy::unwrap_used, reason = "static regex patterns are compile-time validated")]
static OBSERVATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?s)\*\*\u{41d}\u{430}\u{431}\u{43b}\u{44e}\u{434}\u{435}\u{43d}\u{438}\u{435}:\*\*\s*(.*?)(?:\*\*|$)",
    )
    .unwrap()
});

#[expect(clippy::unwrap_used, reason = "static regex patterns are compile-time validated")]
static IMPLICATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\*\*\u{418}\u{43c}\u{43f}\u{43b}\u{438}\u{43a}\u{430}\u{446}\u{438}\u{44f} \u{434}\u{43b}\u{44f} AGI:\*\*\s*(.*?)(?:\*\*|$)").unwrap()
});

#[expect(clippy::unwrap_used, reason = "static regex patterns are compile-time validated")]
static RECOMMENDATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\*\*\u{420}\u{435}\u{43a}\u{43e}\u{43c}\u{435}\u{43d}\u{434}\u{430}\u{446}\u{438}\u{44f}:\*\*\s*(.*?)(?:\*\*|$)").unwrap()
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
    match category.to_lowercase().as_str() {
        "слабость" => KnowledgeType::Gotcha,
        "паттерн" => KnowledgeType::Pattern,
        "missing for agi" => KnowledgeType::Gotcha,
        "неоптимальные решения" => KnowledgeType::Gotcha,
        "планирование" => KnowledgeType::Pattern,
        "ригидность" => KnowledgeType::Gotcha,
        "галлюцинации" => KnowledgeType::Gotcha,
        "позитивный" => KnowledgeType::Pattern,
        _ => KnowledgeType::Gotcha,
    }
}

/// Extract keywords from title for triggers
fn extract_triggers(title: &str) -> Vec<String> {
    title
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Parse markdown content and extract insights
fn parse_insights(content: &str) -> Vec<ParsedInsight> {
    let mut insights = Vec::new();
    let sections: Vec<&str> =
        content.split("### \u{418}\u{43d}\u{441}\u{430}\u{439}\u{442}").collect();

    for section in sections.iter().skip(1) {
        let full_section = format!("### \u{418}\u{43d}\u{441}\u{430}\u{439}\u{442}{section}");

        let title = INSIGHT_RE
            .captures(&full_section)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_owned());

        let category = CATEGORY_RE
            .captures(&full_section)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_owned());

        let observation = OBSERVATION_RE
            .captures(&full_section)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_owned());

        let implication = IMPLICATION_RE
            .captures(&full_section)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_owned());

        let recommendation = RECOMMENDATION_RE
            .captures(&full_section)
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
    let results = storage.search_knowledge(title, 10).await?;
    Ok(results.iter().any(|r| r.knowledge.title == title))
}

/// Import insights from a single file
async fn import_file(storage: &StorageBackend, path: &Path) -> Result<(usize, usize)> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let insights = parse_insights(&content);
    let mut imported = 0;
    let mut skipped = 0;

    for insight in insights {
        if title_exists(storage, &insight.title).await? {
            skipped += 1;
            continue;
        }

        let description = match &insight.implication {
            Some(impl_text) => {
                format!(
                    "{}\n\n\u{418}\u{43c}\u{43f}\u{43b}\u{438}\u{43a}\u{430}\u{446}\u{438}\u{44f} \u{434}\u{43b}\u{44f} AGI: {}",
                    insight.observation, impl_text
                )
            },
            None => insight.observation.clone(),
        };

        let input = KnowledgeInput::new(
            category_to_knowledge_type(&insight.category),
            insight.title.clone(),
            description,
            insight.recommendation,
            extract_triggers(&insight.title),
            Some("agi-audit".to_owned()),
            None,
        );

        storage.save_knowledge(input).await?;
        imported += 1;
    }

    Ok((imported, skipped))
}

/// Run import-insights command
pub(crate) async fn run(file: Option<String>, dir: Option<String>) -> Result<()> {
    let storage = crate::create_storage().await?;

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
                    },
                    Err(e) => {
                        eprintln!("Error processing {}: {}", path.display(), e);
                    },
                }
            }
        }
    }

    println!("\nImported {total_imported} insights, skipped {total_skipped} duplicates");

    Ok(())
}
