use anyhow::Result;
use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_infinite::InfiniteMemory;
use opencode_mem_llm::LlmClient;
use opencode_mem_mcp::run_mcp_server;
use opencode_mem_service::{ObservationService, SessionService};
use opencode_mem_storage::{init_sqlite_vec, Storage};
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::{ensure_db_dir, get_api_key, get_base_url, get_db_path};

pub(crate) async fn run() -> Result<()> {
    init_sqlite_vec();

    let db_path = get_db_path();
    ensure_db_dir(&db_path)?;
    let storage = Arc::new(Storage::new(&db_path)?);

    let api_key = get_api_key()?;
    let llm = Arc::new(LlmClient::new(api_key, get_base_url()));

    let embeddings = match EmbeddingService::new() {
        Ok(emb) => Some(Arc::new(emb)),
        Err(e) => {
            eprintln!("Warning: Embeddings not available: {e}. Semantic search disabled.");
            None
        },
    };

    let infinite_mem = if let Ok(url) = std::env::var("INFINITE_MEMORY_URL") {
        match InfiniteMemory::new(&url, llm.clone()).await {
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

    // Event channel for SSE (MCP doesn't serve SSE, but services require it)
    let (event_tx, _initial_rx) = broadcast::channel(100);

    let observation_service = Arc::new(ObservationService::new(
        storage.clone(),
        llm.clone(),
        infinite_mem.clone(),
        event_tx,
        embeddings.clone(),
    ));
    let session_service =
        Arc::new(SessionService::new(storage.clone(), llm, observation_service.clone()));

    let handle = tokio::runtime::Handle::current();

    tokio::task::spawn_blocking(move || {
        run_mcp_server(
            storage,
            embeddings,
            infinite_mem,
            observation_service,
            session_service,
            handle,
        );
    })
    .await?;

    Ok(())
}
