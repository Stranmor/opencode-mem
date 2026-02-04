use anyhow::Result;
use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_infinite::InfiniteMemory;
use opencode_mem_llm::LlmClient;
use opencode_mem_mcp::run_mcp_server;
use opencode_mem_storage::{init_sqlite_vec, Storage};
use std::sync::Arc;

use crate::{ensure_db_dir, get_api_key, get_base_url, get_db_path};

pub(crate) async fn run() -> Result<()> {
    init_sqlite_vec();

    let db_path = get_db_path();
    ensure_db_dir(&db_path)?;
    let storage = Arc::new(Storage::new(&db_path)?);

    let embeddings = match EmbeddingService::new() {
        Ok(emb) => Some(Arc::new(emb)),
        Err(e) => {
            eprintln!("Warning: Embeddings not available: {e}. Semantic search disabled.");
            None
        },
    };

    let infinite_mem = if let Ok(url) = std::env::var("INFINITE_MEMORY_URL") {
        let api_key = get_api_key()?;
        let llm = Arc::new(LlmClient::new(api_key, get_base_url()));
        match InfiniteMemory::new(&url, llm).await {
            Ok(mem) => {
                eprintln!("Connected to infinite memory: {url}");
                Some(Arc::new(mem))
            },
            Err(e) => {
                eprintln!("Warning: Failed to connect to infinite memory: {e}");
                None
            },
        }
    } else {
        eprintln!("INFINITE_MEMORY_URL not set, infinite memory disabled");
        None
    };

    let handle = tokio::runtime::Handle::current();

    tokio::task::spawn_blocking(move || {
        run_mcp_server(storage, embeddings, infinite_mem, handle);
    })
    .await?;

    Ok(())
}
