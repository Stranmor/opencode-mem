use anyhow::Result;
use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_infinite::InfiniteMemory;
use opencode_mem_llm::LlmClient;
use opencode_mem_mcp::run_mcp_server;
use opencode_mem_service::{KnowledgeService, ObservationService, SearchService, SessionService};
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::{get_api_key, get_base_url};

pub(crate) async fn run() -> Result<()> {
    let storage = Arc::new(crate::create_storage().await?);

    let api_key = get_api_key()?;
    let llm = Arc::new(LlmClient::new(api_key, get_base_url())?);

    let embeddings_disabled = std::env::var("OPENCODE_MEM_DISABLE_EMBEDDINGS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let embeddings = if embeddings_disabled {
        eprintln!("Embeddings disabled via OPENCODE_MEM_DISABLE_EMBEDDINGS");
        None
    } else {
        match EmbeddingService::new() {
            Ok(emb) => Some(Arc::new(emb)),
            Err(e) => {
                eprintln!("Warning: Embeddings not available: {e}. Semantic search disabled.");
                None
            },
        }
    };

    let infinite_mem = if let Ok(url) = std::env::var("INFINITE_MEMORY_URL")
        .or_else(|_| std::env::var("OPENCODE_MEM_INFINITE_MEMORY"))
    {
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await;

        match pool {
            Ok(p) => match InfiniteMemory::new(p, llm.clone()).await {
                Ok(mem) => {
                    eprintln!("Connected to infinite memory");
                    Some(Arc::new(mem))
                },
                Err(e) => {
                    eprintln!("Warning: Failed to connect to infinite memory: {e}");
                    None
                },
            },
            Err(e) => {
                eprintln!("Warning: Failed to connect to infinite memory database: {}", e);
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
    let knowledge_service = Arc::new(KnowledgeService::new(storage.clone()));
    let search_service = Arc::new(SearchService::new(storage, embeddings));

    let handle = tokio::runtime::Handle::current();

    run_mcp_server(
        infinite_mem,
        observation_service,
        session_service,
        knowledge_service,
        search_service,
        handle,
    )
    .await;

    Ok(())
}
