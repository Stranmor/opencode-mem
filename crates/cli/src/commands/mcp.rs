use anyhow::Result;
use opencode_mem_core::AppConfig;
use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_llm::LlmClient;
use opencode_mem_mcp::run_mcp_server;
use opencode_mem_service::{
    InfiniteMemoryService, KnowledgeService, ObservationService, SearchService, SessionService,
};
use std::sync::Arc;
use tokio::sync::broadcast;

pub(crate) async fn run(config: Arc<AppConfig>) -> Result<()> {
    opencode_mem_storage::init_queue_config(config.max_retry, config.visibility_timeout_secs);
    opencode_mem_service::init_compression_config(
        config.max_content_chars,
        config.max_total_chars,
        config.max_events,
    );
    let storage = Arc::new(crate::create_storage(&config.database_url).await?);

    let llm = Arc::new(LlmClient::new(
        config.api_key.clone(),
        config.api_url.clone(),
        config.model.clone(),
    )?);

    let embeddings = if config.disable_embeddings {
        eprintln!("Embeddings disabled via OPENCODE_MEM_DISABLE_EMBEDDINGS");
        None
    } else {
        match EmbeddingService::new(config.embedding_threads) {
            Ok(emb) => Some(Arc::new(emb)),
            Err(e) => {
                eprintln!("Warning: Embeddings not available: {e}. Semantic search disabled.");
                None
            }
        }
    };

    let infinite_mem = if let Some(ref url) = config.infinite_memory_url {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(
                opencode_mem_core::PG_POOL_ACQUIRE_TIMEOUT_SECS,
            ))
            .connect_lazy(url);

        match pool {
            Ok(p) => match InfiniteMemoryService::new(p, llm.clone()).await {
                Ok(mem) => {
                    eprintln!("Connected to infinite memory");
                    Some(Arc::new(mem))
                }
                Err(e) => {
                    eprintln!("Warning: Failed to initialize infinite memory: {e}");
                    None
                }
            },
            Err(e) => {
                eprintln!("Warning: Failed to create infinite memory pool: {e}");
                None
            }
        }
    } else {
        eprintln!("INFINITE_MEMORY_URL not set, infinite memory disabled");
        None
    };

    let (event_tx, _) = broadcast::channel(100);

    let observation_service = Arc::new(ObservationService::new(
        storage.clone(),
        llm.clone(),
        infinite_mem.clone(),
        event_tx,
        embeddings.clone(),
        &config,
    ));
    let session_service = Arc::new(SessionService::new(storage.clone(), llm.clone()));
    let knowledge_service = Arc::new(KnowledgeService::new(storage.clone()));
    let search_service = Arc::new(SearchService::new(
        storage,
        embeddings,
        infinite_mem.clone(),
        config.dedup_threshold,
    ));

    let handle = tokio::runtime::Handle::current();

    let pending_writes = Arc::new(opencode_mem_service::PendingWriteQueue::new());

    run_mcp_server(
        infinite_mem,
        observation_service,
        session_service,
        knowledge_service,
        search_service,
        pending_writes,
        handle,
    )
    .await;

    Ok(())
}
