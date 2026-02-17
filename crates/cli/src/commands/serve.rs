use anyhow::Result;
use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_http::{
    create_router, run_startup_recovery, start_background_processor, start_compression_pipeline,
    AppState, Settings,
};
use opencode_mem_infinite::InfiniteMemory;
use opencode_mem_llm::LlmClient;
use opencode_mem_service::{
    KnowledgeService, ObservationService, QueueService, SearchService, SessionService,
};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock, Semaphore};

use crate::{get_api_key, get_base_url};

pub(crate) async fn run(port: u16, host: String) -> Result<()> {
    let storage = Arc::new(crate::create_storage().await?);

    let api_key = get_api_key()?;
    let llm = Arc::new(LlmClient::new(api_key.clone(), get_base_url())?);
    // Initial receiver dropped - subscribers use event_tx.subscribe()
    let (event_tx, _initial_rx) = broadcast::channel(100);

    let infinite_mem = if let Ok(url) = std::env::var("INFINITE_MEMORY_URL") {
        match InfiniteMemory::new(&url, llm.clone()).await {
            Ok(mem) => {
                tracing::info!("Connected to infinite memory: {}", url);
                Some(Arc::new(mem))
            },
            Err(e) => {
                tracing::warn!("Failed to connect to infinite memory: {}", e);
                None
            },
        }
    } else {
        tracing::info!("INFINITE_MEMORY_URL not set, infinite memory disabled");
        None
    };

    let embeddings = match EmbeddingService::new() {
        Ok(emb) => {
            tracing::info!("Embedding service initialized (BGE-M3, 1024 dimensions)");
            Some(Arc::new(emb))
        },
        Err(e) => {
            tracing::warn!("Failed to initialize embeddings: {}", e);
            None
        },
    };

    let observation_service = Arc::new(ObservationService::new(
        storage.clone(),
        llm.clone(),
        infinite_mem.clone(),
        event_tx.clone(),
        embeddings.clone(),
    ));
    let session_service =
        Arc::new(SessionService::new(storage.clone(), llm.clone(), observation_service.clone()));
    let knowledge_service = Arc::new(KnowledgeService::new(storage.clone()));
    let search_service = Arc::new(SearchService::new(storage.clone(), embeddings.clone()));
    let queue_service = Arc::new(QueueService::new(storage.clone()));

    let state = Arc::new(AppState {
        storage,
        llm: llm.clone(),
        semaphore: Arc::new(Semaphore::new(10)),
        event_tx,
        processing_active: AtomicBool::new(true),
        settings: RwLock::new(Settings::default()),
        infinite_mem,
        observation_service,
        session_service,
        knowledge_service,
        search_service,
        queue_service,
        embeddings,
    });

    if let Err(e) = run_startup_recovery(&state).await {
        tracing::warn!("Startup recovery failed: {}", e);
    }

    start_background_processor(state.clone());

    if let Some(ref infinite_mem) = state.infinite_mem {
        start_compression_pipeline(Arc::clone(infinite_mem));
        tracing::info!("Compression pipeline started (every 5 minutes)");
    }

    let router = create_router(state);
    let addr = format!("{host}:{port}");
    tracing::info!("Starting HTTP server on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
