use anyhow::Result;
use opencode_mem_embeddings::{EmbeddingProvider as _, EmbeddingService};
use opencode_mem_storage::traits::{EmbeddingStore, ObservationStore, SearchStore, StatsStore};
use std::collections::HashSet;

pub(crate) async fn run_search(
    query: String,
    limit: usize,
    project: Option<String>,
    obs_type: Option<String>,
) -> Result<()> {
    let storage = crate::create_storage().await?;
    let results = storage
        .search_with_filters(
            Some(&query),
            project.as_deref(),
            obs_type.as_deref(),
            None,
            None,
            limit,
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

pub(crate) async fn run_stats() -> Result<()> {
    let storage = crate::create_storage().await?;
    let stats = storage.get_stats().await?;
    println!("{}", serde_json::to_string_pretty(&stats)?);
    Ok(())
}

pub(crate) async fn run_projects() -> Result<()> {
    let storage = crate::create_storage().await?;
    let projects = storage.get_all_projects().await?;
    println!("{}", serde_json::to_string_pretty(&projects)?);
    Ok(())
}

pub(crate) async fn run_recent(limit: usize) -> Result<()> {
    let storage = crate::create_storage().await?;
    let results = storage.get_recent(limit).await?;
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

pub(crate) async fn run_get(id: String) -> Result<()> {
    let storage = crate::create_storage().await?;
    match storage.get_by_id(&id).await? {
        Some(obs) => println!("{}", serde_json::to_string_pretty(&obs)?),
        None => println!("Observation not found: {id}"),
    }
    Ok(())
}

pub(crate) async fn run_backfill_embeddings(batch_size: usize) -> Result<()> {
    let storage = crate::create_storage().await?;

    println!("Initializing embedding model (first run downloads ~100MB)...");
    let embeddings = EmbeddingService::new()?;

    let mut total = 0;
    let mut failed_ids: HashSet<String> = HashSet::new();
    loop {
        let all_observations = storage.get_observations_without_embeddings(batch_size).await?;
        if all_observations.is_empty() {
            break;
        }
        let observations: Vec<_> =
            all_observations.into_iter().filter(|obs| !failed_ids.contains(&obs.id)).collect();
        if observations.is_empty() {
            eprintln!(
                "Remaining {} observations all previously failed — stopping",
                failed_ids.len()
            );
            break;
        }

        for obs in &observations {
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
                        failed_ids.insert(obs.id.clone());
                    } else {
                        total += 1;
                        if total % 10 == 0 {
                            println!("Processed {total} observations...");
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Failed to generate embedding for {}: {}", obs.id, e);
                    failed_ids.insert(obs.id.clone());
                },
            }
        }
    }

    if !failed_ids.is_empty() {
        eprintln!("Warning: {} observations failed to process", failed_ids.len());
    }
    println!("Backfill complete. Generated embeddings for {total} observations.");
    Ok(())
}
