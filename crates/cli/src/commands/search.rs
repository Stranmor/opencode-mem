use anyhow::Result;
use opencode_mem_embeddings::{EmbeddingProvider, EmbeddingService};
use opencode_mem_storage::{init_sqlite_vec, Storage};
use std::collections::HashSet;

use crate::{ensure_db_dir, get_db_path};

pub(crate) fn run_search(
    query: String,
    limit: usize,
    project: Option<String>,
    obs_type: Option<String>,
) -> Result<()> {
    let db_path = get_db_path();
    ensure_db_dir(&db_path)?;
    let storage = Storage::new(&db_path)?;
    let results = storage.search_with_filters(
        Some(&query),
        project.as_deref(),
        obs_type.as_deref(),
        None,
        None,
        limit,
    )?;
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

pub(crate) fn run_stats() -> Result<()> {
    let db_path = get_db_path();
    ensure_db_dir(&db_path)?;
    let storage = Storage::new(&db_path)?;
    let stats = storage.get_stats()?;
    println!("{}", serde_json::to_string_pretty(&stats)?);
    Ok(())
}

pub(crate) fn run_projects() -> Result<()> {
    let db_path = get_db_path();
    ensure_db_dir(&db_path)?;
    let storage = Storage::new(&db_path)?;
    let projects = storage.get_all_projects()?;
    println!("{}", serde_json::to_string_pretty(&projects)?);
    Ok(())
}

pub(crate) fn run_recent(limit: usize) -> Result<()> {
    let db_path = get_db_path();
    ensure_db_dir(&db_path)?;
    let storage = Storage::new(&db_path)?;
    let results = storage.get_recent(limit)?;
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

pub(crate) fn run_get(id: String) -> Result<()> {
    let db_path = get_db_path();
    ensure_db_dir(&db_path)?;
    let storage = Storage::new(&db_path)?;
    match storage.get_by_id(&id)? {
        Some(obs) => println!("{}", serde_json::to_string_pretty(&obs)?),
        None => println!("Observation not found: {id}"),
    }
    Ok(())
}

pub(crate) fn run_backfill_embeddings(batch_size: usize) -> Result<()> {
    init_sqlite_vec();

    let db_path = get_db_path();
    ensure_db_dir(&db_path)?;
    let storage = Storage::new(&db_path)?;

    println!("Initializing embedding model (first run downloads ~100MB)...");
    let embeddings = EmbeddingService::new()?;

    let mut total = 0;
    let mut failed_ids: HashSet<String> = HashSet::new();
    loop {
        let observations = storage.get_observations_without_embeddings(batch_size)?;
        let observations: Vec<_> =
            observations.into_iter().filter(|obs| !failed_ids.contains(&obs.id)).collect();
        if observations.is_empty() {
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
                    if let Err(e) = storage.store_embedding(&obs.id, &vec) {
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
