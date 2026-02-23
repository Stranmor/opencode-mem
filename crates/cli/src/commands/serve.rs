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
    // Initial receiver dropped immediately - subscribers use event_tx.subscribe()
    let (event_tx, _) = broadcast::channel(100);

    let infinite_mem = if let Ok(url) = std::env::var("INFINITE_MEMORY_URL")
        .or_else(|_| std::env::var("OPENCODE_MEM_INFINITE_MEMORY"))
    {
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(5).connect(&url).await;

        match pool {
            Ok(p) => match InfiniteMemory::new(p, llm.clone()).await {
                Ok(mem) => {
                    tracing::info!("Connected to infinite memory");
                    Some(Arc::new(mem))
                },
                Err(e) => {
                    tracing::warn!("Failed to initialize infinite memory migrations: {}", e);
                    None
                },
            },
            Err(e) => {
                tracing::warn!("Failed to connect to infinite memory database: {}", e);
                None
            },
        }
    } else {
        tracing::info!("INFINITE_MEMORY_URL not set, infinite memory disabled");
        None
    };

    let embeddings_disabled = std::env::var("OPENCODE_MEM_DISABLE_EMBEDDINGS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let embeddings = if embeddings_disabled {
        tracing::info!("Embeddings disabled via OPENCODE_MEM_DISABLE_EMBEDDINGS");
        None
    } else {
        match EmbeddingService::new() {
            Ok(emb) => {
                tracing::info!("Embedding service initialized (BGE-M3, 1024 dimensions)");
                Some(Arc::new(emb))
            },
            Err(e) => {
                tracing::warn!("Failed to initialize embeddings: {}", e);
                None
            },
        }
    };

    let observation_service = Arc::new(ObservationService::new(
        storage.clone(),
        llm.clone(),
        infinite_mem.clone(),
        event_tx.clone(),
        embeddings.clone(),
    ));
    let session_service = Arc::new(SessionService::new(storage.clone(), llm.clone()));
    let knowledge_service = Arc::new(KnowledgeService::new(storage.clone()));
    let search_service = Arc::new(SearchService::new(storage.clone(), embeddings.clone()));
    let queue_service = Arc::new(QueueService::new(storage.clone()));

    let state = Arc::new(AppState {
        semaphore: Arc::new(Semaphore::new(opencode_mem_core::env_parse_with_default(
            "OPENCODE_MEM_QUEUE_WORKERS",
            10,
        ))),
        event_tx,
        processing_active: AtomicBool::new(true),
        settings: RwLock::new(Settings::default()),
        infinite_mem,
        observation_service,
        session_service,
        knowledge_service,
        search_service,
        queue_service,
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
    let addr_str = format!("{host}:{port}");
    let addr: std::net::SocketAddr = addr_str.parse()?;

    let domain = if addr.is_ipv6() { socket2::Domain::IPV6 } else { socket2::Domain::IPV4 };

    let socket = socket2::Socket::new(domain, socket2::Type::STREAM, None)?;

    if let Err(e) = socket.set_reuse_address(true) {
        tracing::warn!("Failed to set SO_REUSEADDR: {}", e);
    }

    #[cfg(target_family = "unix")]
    {
        if let Err(e) = socket.set_reuse_port(true) {
            tracing::warn!("Failed to set SO_REUSEPORT: {}", e);
        }
    }

    socket.bind(&addr.into())?;
    socket.listen(1024)?;

    let std_listener: std::net::TcpListener = socket.into();
    std_listener.set_nonblocking(true)?;

    let listener = tokio::net::TcpListener::from_std(std_listener)?;

    tracing::info!("Starting HTTP server on {}", addr_str);
    axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .await?;

    Ok(())
}
