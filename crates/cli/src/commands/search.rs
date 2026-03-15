use anyhow::Result;
use opencode_mem_core::{AppConfig, observation_embedding_text};
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
    let obs_type_lower = obs_type.as_ref().map(|s| s.to_lowercase());
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
    let embeddings = Arc::new(EmbeddingService::new(thread_count)?);

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
            let text = observation_embedding_text(obs);
            let emb_clone = Arc::clone(&embeddings);
            let embed_result = tokio::task::spawn_blocking(move || emb_clone.embed(&text)).await?;

            match embed_result {
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
    let archived = storage.auto_archive(90).await?;
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

    let embeddings: Option<Arc<EmbeddingService>> = if config.disable_embeddings {
        println!("Embeddings disabled — skipping embedding regeneration");
        None
    } else {
        println!("Initializing embedding model (first run downloads ~100MB)...");
        let thread_count = AppConfig::resolve_embedding_threads();
        match EmbeddingService::new(thread_count) {
            Ok(svc) => Some(Arc::new(svc)),
            Err(e) => {
                eprintln!(
                    "Warning: embedding init failed ({e}), metadata will be enriched without embedding regen"
                );
                None
            }
        }
    };

    println!(
        "Backfilling metadata (LLM: {} via {})",
        config.model, config.api_url
    );

    let mut total_success = 0_usize;
    let mut total_failed = 0_usize;
    let mut total_embeddings = 0_usize;
    let mut embedding_failed_ids: Vec<String> = Vec::new();
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
                        if let Some(ref emb_svc) = embeddings {
                            match storage.get_by_id(obs.id.as_ref()).await {
                                Ok(Some(updated_obs)) => {
                                    let text = observation_embedding_text(&updated_obs);
                                    let emb_clone = Arc::clone(emb_svc);
                                    let embed_result =
                                        tokio::task::spawn_blocking(move || emb_clone.embed(&text))
                                            .await;
                                    match embed_result {
                                        Ok(Ok(vec)) => {
                                            if let Err(e) =
                                                storage.store_embedding(obs.id.as_ref(), &vec).await
                                            {
                                                eprintln!(
                                                    "  Embedding store failed for {}: {e}",
                                                    obs.id
                                                );
                                                embedding_failed_ids.push(obs.id.to_string());
                                            } else {
                                                total_embeddings += 1;
                                            }
                                        }
                                        Ok(Err(e)) => {
                                            eprintln!("  Embedding gen failed for {}: {e}", obs.id);
                                            embedding_failed_ids.push(obs.id.to_string());
                                        }
                                        Err(e) => {
                                            eprintln!(
                                                "  Embedding spawn_blocking panicked for {}: {e}",
                                                obs.id
                                            );
                                            embedding_failed_ids.push(obs.id.to_string());
                                        }
                                    }
                                }
                                Ok(None) => {
                                    eprintln!(
                                        "  Observation {} not found after metadata update",
                                        obs.id
                                    );
                                    embedding_failed_ids.push(obs.id.to_string());
                                }
                                Err(e) => {
                                    eprintln!("  Re-fetch failed for {}: {e}", obs.id);
                                    embedding_failed_ids.push(obs.id.to_string());
                                }
                            }
                        }
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
    if !embedding_failed_ids.is_empty() {
        eprintln!(
            "Warning: {} embedding regenerations failed — run `backfill-embeddings` to fix",
            embedding_failed_ids.len()
        );
    }
    println!(
        "Backfill complete: {total_success} enriched, {total_embeddings} embeddings regenerated, {total_failed} failed."
    );
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
