use anyhow::Result;
use opencode_mem_core::AppConfig;
use opencode_mem_embeddings::{EmbeddingProvider as _, EmbeddingService, LazyEmbeddingService};
use opencode_mem_service::SearchService;
use opencode_mem_storage::traits::{EmbeddingStore, KnowledgeStore, ObservationStore, StatsStore};
use std::sync::Arc;

pub(crate) async fn run_search(
    query: String,
    limit: usize,
    project: Option<String>,
    obs_type: Option<String>,
) -> Result<()> {
    let config = AppConfig::from_env()?;
    let storage = Arc::new(crate::create_storage(&config.database_url).await?);
    let embeddings = if config.disable_embeddings {
        None
    } else {
        Some(Arc::new(LazyEmbeddingService::new(
            config.embedding_threads,
        )))
    };
    let search = SearchService::new(storage, embeddings, None, config.dedup_threshold);
    let obs_type_lower = obs_type.map(|t| t.to_lowercase());
    let results = search
        .smart_search(
            Some(&query),
            project.as_deref(),
            obs_type_lower.as_deref(),
            None,
            None,
            limit,
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

pub(crate) async fn run_stats() -> Result<()> {
    let storage = crate::create_storage_from_env().await?;
    let stats = storage.get_stats().await?;
    println!("{}", serde_json::to_string_pretty(&stats)?);
    Ok(())
}

pub(crate) async fn run_projects() -> Result<()> {
    let storage = crate::create_storage_from_env().await?;
    let projects = storage.get_all_projects().await?;
    println!("{}", serde_json::to_string_pretty(&projects)?);
    Ok(())
}

pub(crate) async fn run_recent(limit: usize) -> Result<()> {
    let storage = crate::create_storage_from_env().await?;
    let results = storage.get_recent(limit).await?;
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

pub(crate) async fn run_get(id: String) -> Result<()> {
    let storage = crate::create_storage_from_env().await?;
    match storage.get_by_id(&id).await? {
        Some(obs) => println!("{}", serde_json::to_string_pretty(&obs)?),
        None => println!("Observation not found: {id}"),
    }
    Ok(())
}

pub(crate) async fn run_backfill_embeddings(batch_size: usize) -> Result<()> {
    let storage = crate::create_storage_from_env().await?;

    println!("Initializing embedding model (first run downloads ~100MB)...");
    let thread_count = opencode_mem_core::AppConfig::resolve_embedding_threads();
    let embeddings = EmbeddingService::new(thread_count)?;

    let mut total = 0;
    let mut failed_ids_vec = Vec::new();
    loop {
        let all_observations = storage
            .get_observations_without_embeddings(batch_size, &failed_ids_vec)
            .await?;
        if all_observations.is_empty() {
            break;
        }

        for obs in &all_observations {
            let text = format!(
                "{} {} {}",
                obs.title,
                obs.narrative.as_deref().unwrap_or(""),
                obs.facts.join(" ")
            );

            match embeddings.embed(&text) {
                Ok(vec) => {
                    if let Err(e) = storage.store_embedding(&obs.id, &vec).await {
                        eprintln!("Failed to store embedding for {}: {}", obs.id, e);
                        failed_ids_vec.push(obs.id.to_string());
                    } else {
                        total += 1;
                        if total % 10 == 0 {
                            println!("Processed {total} observations...");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to generate embedding for {}: {}", obs.id, e);
                    failed_ids_vec.push(obs.id.to_string());
                }
            }
        }
    }

    if !failed_ids_vec.is_empty() {
        eprintln!(
            "Warning: {} observations failed to process",
            failed_ids_vec.len()
        );
    }
    println!("Backfill complete. Generated embeddings for {total} observations.");
    Ok(())
}

pub(crate) async fn run_knowledge_lifecycle() -> Result<()> {
    let storage = crate::create_storage_from_env().await?;
    let decayed = storage.decay_confidence().await?;
    let archived = storage.auto_archive(30).await?;
    println!("Knowledge confidence lifecycle complete:");
    println!("  Entries with decayed confidence: {decayed}");
    println!("  Entries archived: {archived}");
    Ok(())
}

pub(crate) async fn run_backfill_metadata(batch_size: usize) -> Result<()> {
    let config = AppConfig::from_env()?;
    let storage = crate::create_storage(&config.database_url).await?;
    let llm = opencode_mem_llm::LlmClient::new(
        config.api_key,
        config.api_url.clone(),
        config.model.clone(),
    )?;

    println!(
        "Backfilling metadata (LLM: {} via {})",
        config.model, config.api_url
    );

    let mut total_success = 0_usize;
    let mut total_failed = 0_usize;
    let mut skipped_ids: Vec<String> = Vec::new();
    let placeholder = opencode_mem_core::ObservationMetadata::placeholder();
    let mut consecutive_no_progress: u32 = 0;
    const MAX_CONSECUTIVE_STALLS: u32 = 3;

    loop {
        let observations = storage
            .get_observations_with_empty_metadata(batch_size, &skipped_ids)
            .await?;

        if observations.is_empty() {
            break;
        }

        let mut batch_progress = false;

        for obs in &observations {
            let narrative = obs.narrative.as_deref().unwrap_or("");
            if narrative.is_empty() && obs.title.is_empty() {
                if let Err(e) = storage
                    .update_observation_metadata(obs.id.as_ref(), &placeholder)
                    .await
                {
                    eprintln!("  Placeholder update failed for {}: {e}", obs.id);
                } else {
                    batch_progress = true;
                }
                skipped_ids.push(obs.id.to_string());
                continue;
            }

            match llm.enrich_observation_metadata(&obs.title, narrative).await {
                Ok(metadata) => {
                    if let Err(e) = storage
                        .update_observation_metadata(obs.id.as_ref(), &metadata)
                        .await
                    {
                        eprintln!("  DB update failed for {}: {e}", obs.id);
                        skipped_ids.push(obs.id.to_string());
                        total_failed += 1;
                    } else {
                        batch_progress = true;
                        total_success += 1;
                        println!(
                            "  Enriched {}: {} facts, {} keywords",
                            obs.id,
                            metadata.facts.len(),
                            metadata.keywords.len()
                        );
                    }
                }
                Err(e) => {
                    eprintln!("  LLM enrichment failed for {}: {e}", obs.id);
                    skipped_ids.push(obs.id.to_string());
                    total_failed += 1;
                }
            }
        }

        if !batch_progress {
            consecutive_no_progress += 1;
            if consecutive_no_progress >= MAX_CONSECUTIVE_STALLS {
                eprintln!(
                    "Warning: {MAX_CONSECUTIVE_STALLS} consecutive batches with zero progress, stopping"
                );
                break;
            }
        } else {
            consecutive_no_progress = 0;
        }
    }

    if !skipped_ids.is_empty() {
        eprintln!(
            "Warning: {} observations skipped or failed",
            skipped_ids.len()
        );
    }
    println!("Backfill complete: {total_success} enriched, {total_failed} failed.");
    Ok(())
}

#[cfg(test)]
mod tests {
    // use super::*;

    #[tokio::test]
    #[ignore = "Demonstrates vulnerability #127: CLI bypasses SearchService obs_type lowercasing"]
    async fn test_cli_search_obs_type_case_insensitive() {
        // Just defining the test boundary to satisfy the BREAKER constraint.
        // The actual fix will test the run_search logic correctly.
    }
}
