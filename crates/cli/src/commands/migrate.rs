//! SQLite → PostgreSQL migration command.
//!
//! Copies all data from the local SQLite database to a PostgreSQL instance.
//! Idempotent: skips duplicate observations, upserts sessions/knowledge/summaries.

#[cfg(all(feature = "sqlite", feature = "postgres"))]
pub(crate) async fn run() -> anyhow::Result<()> {
    use std::collections::HashSet;

    use opencode_mem_core::KnowledgeInput;
    use opencode_mem_storage::{
        KnowledgeStore, ObservationStore, SessionStore, StatsStore, StorageBackend, SummaryStore,
    };

    let db_path = crate::get_db_path();
    crate::ensure_db_dir(&db_path)?;
    let sqlite = StorageBackend::new_sqlite(&db_path)?;

    let pg_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set for migration target"))?;
    let pg = StorageBackend::new_postgres(&pg_url).await?;

    // ── 1. Observations ──────────────────────────────────────────
    println!("Migrating observations...");
    let mut offset: usize = 0;
    let batch_size: usize = 500;
    let mut obs_inserted: usize = 0;
    let mut obs_skipped: usize = 0;
    let mut session_ids = HashSet::<String>::new();

    loop {
        let page =
            StatsStore::get_observations_paginated(&sqlite, offset, batch_size, None).await?;
        if page.items.is_empty() {
            break;
        }
        for obs in &page.items {
            session_ids.insert(obs.session_id.clone());
            match ObservationStore::save_observation(&pg, obs).await {
                Ok(true) => obs_inserted += 1,
                Ok(false) => obs_skipped += 1,
                Err(err) => {
                    tracing::warn!(id = %obs.id, "Failed to migrate observation: {err}");
                    obs_skipped += 1;
                },
            }
        }
        let fetched = page.items.len();
        offset += fetched;
        if fetched < batch_size {
            break;
        }
    }
    println!("  observations: {obs_inserted} inserted, {obs_skipped} skipped (total {offset})");

    // ── 2. Sessions ──────────────────────────────────────────────
    println!("Migrating sessions...");
    let mut sess_migrated: usize = 0;
    let mut sess_skipped: usize = 0;

    for sid in &session_ids {
        // observation.session_id is content_session_id, not the sessions table PK
        match SessionStore::get_session_by_content_id(&sqlite, sid).await? {
            Some(session) => {
                if let Err(err) = SessionStore::save_session(&pg, &session).await {
                    tracing::warn!(id = %sid, "Failed to migrate session: {err}");
                    sess_skipped += 1;
                } else {
                    sess_migrated += 1;
                }
            },
            None => {
                sess_skipped += 1;
            },
        }
    }
    println!("  sessions: {sess_migrated} migrated, {sess_skipped} skipped");

    // ── 3. Knowledge ─────────────────────────────────────────────
    println!("Migrating knowledge...");
    let all_knowledge = KnowledgeStore::list_knowledge(&sqlite, None, 100_000).await?;
    let mut know_migrated: usize = 0;
    let mut know_skipped: usize = 0;

    for gk in &all_knowledge {
        let input = KnowledgeInput::new(
            gk.knowledge_type,
            gk.title.clone(),
            gk.description.clone(),
            gk.instructions.clone(),
            gk.triggers.clone(),
            gk.source_projects.first().cloned(),
            gk.source_observations.first().cloned(),
        );
        match KnowledgeStore::save_knowledge(&pg, input).await {
            Ok(_) => know_migrated += 1,
            Err(err) => {
                tracing::warn!(title = %gk.title, "Failed to migrate knowledge: {err}");
                know_skipped += 1;
            },
        }
    }
    println!(
        "  knowledge: {know_migrated} migrated, {know_skipped} skipped (total {})",
        all_knowledge.len()
    );

    // ── 4. Session Summaries ─────────────────────────────────────
    println!("Migrating session summaries...");
    let mut sum_offset: usize = 0;
    let mut sum_migrated: usize = 0;
    let mut sum_skipped: usize = 0;

    loop {
        let page =
            SummaryStore::get_summaries_paginated(&sqlite, sum_offset, batch_size, None).await?;
        if page.items.is_empty() {
            break;
        }
        for summary in &page.items {
            if let Err(err) = SummaryStore::save_summary(&pg, summary).await {
                tracing::warn!(session = %summary.session_id, "Failed to migrate summary: {err}");
                sum_skipped += 1;
            } else {
                sum_migrated += 1;
            }
        }
        let fetched = page.items.len();
        sum_offset += fetched;
        if fetched < batch_size {
            break;
        }
    }
    println!("  summaries: {sum_migrated} migrated, {sum_skipped} skipped");

    println!("\nMigration complete!");
    Ok(())
}
