//! Import AGI audit insights from markdown files into knowledge base

use anyhow::{Context, Result};
use opencode_mem_core::{KnowledgeInput, KnowledgeType};
use opencode_mem_storage::Storage;
use regex::Regex;
use std::path::Path;

use crate::{ensure_db_dir, get_db_path};

/// Parsed insight from markdown
struct ParsedInsight {
    title: String,
    category: String,
    observation: String,
    implication: Option<String>,
    recommendation: Option<String>,
}

/// Map Russian category to KnowledgeType
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
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Parse markdown content and extract insights
fn parse_insights(content: &str) -> Vec<ParsedInsight> {
    let insight_re = Regex::new(r"###\s*Инсайт\s*\d+:\s*\[([^\]]+)\]").unwrap();
    let category_re = Regex::new(r"\*\*Категория:\*\*\s*(.+)").unwrap();
    let observation_re = Regex::new(r"\*\*Наблюдение:\*\*\s*(.+)").unwrap();
    let implication_re = Regex::new(r"\*\*Импликация для AGI:\*\*\s*(.+)").unwrap();
    let recommendation_re = Regex::new(r"\*\*Рекомендация:\*\*\s*(.+)").unwrap();

    let mut insights = Vec::new();
    let sections: Vec<&str> = content.split("### Инсайт").collect();

    for section in sections.iter().skip(1) {
        let full_section = format!("### Инсайт{}", section);

        let title = insight_re
            .captures(&full_section)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string());

        let category = category_re
            .captures(&full_section)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string());

        let observation = observation_re
            .captures(&full_section)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string());

        let implication = implication_re
            .captures(&full_section)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string());

        let recommendation = recommendation_re
            .captures(&full_section)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string());

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
fn title_exists(storage: &Storage, title: &str) -> bool {
    storage
        .search_knowledge(title, 10)
        .map(|results| results.iter().any(|r| r.knowledge.title == title))
        .unwrap_or(false)
}

/// Import insights from a single file
fn import_file(storage: &Storage, path: &Path) -> Result<(usize, usize)> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let insights = parse_insights(&content);
    let mut imported = 0;
    let mut skipped = 0;

    for insight in insights {
        if title_exists(storage, &insight.title) {
            skipped += 1;
            continue;
        }

        let description = match &insight.implication {
            Some(impl_text) => format!(
                "{}\n\nИмпликация для AGI: {}",
                insight.observation, impl_text
            ),
            None => insight.observation.clone(),
        };

        let input = KnowledgeInput {
            knowledge_type: category_to_knowledge_type(&insight.category),
            title: insight.title.clone(),
            description,
            instructions: insight.recommendation,
            triggers: extract_triggers(&insight.title),
            source_project: Some("agi-audit".to_string()),
            source_observation: None,
        };

        storage.save_knowledge(input)?;
        imported += 1;
    }

    Ok((imported, skipped))
}

/// Run import-insights command
pub fn run(file: Option<String>, dir: Option<String>) -> Result<()> {
    let db_path = get_db_path();
    ensure_db_dir(&db_path)?;
    let storage = Storage::new(&db_path)?;

    let mut total_imported = 0;
    let mut total_skipped = 0;

    if let Some(file_path) = file {
        let path = Path::new(&file_path);
        let (imported, skipped) = import_file(&storage, path)?;
        total_imported += imported;
        total_skipped += skipped;
        println!("Processed: {}", path.display());
    }

    if let Some(dir_path) = dir {
        let dir = Path::new(&dir_path);
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                match import_file(&storage, &path) {
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

    println!(
        "\nImported {} insights, skipped {} duplicates",
        total_imported, total_skipped
    );

    Ok(())
}
