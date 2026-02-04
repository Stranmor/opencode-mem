use anyhow::Result;
use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_mcp::run_mcp_server;
use opencode_mem_storage::{init_sqlite_vec, Storage};
use std::sync::Arc;

use crate::{ensure_db_dir, get_db_path};

pub async fn run() -> Result<()> {
    init_sqlite_vec();

    let db_path = get_db_path();
    ensure_db_dir(&db_path)?;
    let storage = Storage::new(&db_path)?;

    let embeddings = match EmbeddingService::new() {
        Ok(emb) => Some(Arc::new(emb)),
        Err(e) => {
            eprintln!(
                "Warning: Embeddings not available: {}. Semantic search disabled.",
                e
            );
            None
        }
    };

    tokio::task::spawn_blocking(move || {
        run_mcp_server(Arc::new(storage), embeddings);
    })
    .await?;

    Ok(())
}
