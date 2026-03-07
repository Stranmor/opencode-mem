use anyhow::Result;
use opencode_mem_core::AppConfig;
use opencode_mem_embeddings::EmbeddingService;
use opencode_mem_http::{
    AppState, Settings, create_router, run_startup_recovery, start_background_processor,
};
use opencode_mem_llm::LlmClient;
use opencode_mem_service::{
    InfiniteMemoryService, KnowledgeService, ObservationService, QueueService, SearchService,
    SessionService,
};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use tokio::sync::{RwLock, Semaphore, broadcast};

pub(crate) async fn run(port: u16, host: String, config: Arc<AppConfig>) -> Result<()> {
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
    let (event_tx, _) = broadcast::channel(100);

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
                    tracing::info!("Connected to infinite memory");
                    Some(Arc::new(mem))
                }
                Err(e) => {
                    tracing::warn!("Failed to initialize infinite memory: {}", e);
                    None
                }
            },
            Err(e) => {
                tracing::warn!("Failed to create infinite memory pool: {}", e);
                None
            }
        }
    } else {
        tracing::info!("INFINITE_MEMORY_URL not set, infinite memory disabled");
        None
    };

    let embeddings = if config.disable_embeddings {
        tracing::info!("Embeddings disabled via OPENCODE_MEM_DISABLE_EMBEDDINGS");
        None
    } else {
        match EmbeddingService::new() {
            Ok(emb) => {
                tracing::info!("Embedding service initialized (BGE-M3, 1024 dimensions)");
                Some(Arc::new(emb))
            }
            Err(e) => {
                tracing::warn!("Failed to initialize embeddings: {}", e);
                None
            }
        }
    };

    let pending_writes = Arc::new(opencode_mem_service::PendingWriteQueue::new());

    let observation_service = Arc::new(ObservationService::new(
        storage.clone(),
        llm.clone(),
        infinite_mem.clone(),
        event_tx.clone(),
        embeddings.clone(),
        &config,
    ));
    let session_service = Arc::new(SessionService::new(storage.clone(), llm.clone()));
    let knowledge_service = Arc::new(KnowledgeService::new(storage.clone()));
    let search_service = Arc::new(SearchService::new(
        storage.clone(),
        embeddings.clone(),
        infinite_mem.clone(),
    ));
    let queue_service = Arc::new(QueueService::new(storage.clone(), pending_writes.clone()));
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel(1);

    let state = Arc::new(AppState {
        semaphore: Arc::new(Semaphore::new(config.queue_workers)),
        event_tx,
        processing_active: AtomicBool::new(true),
        settings: RwLock::new(Settings::default()),
        infinite_mem,
        observation_service,
        session_service,
        knowledge_service,
        search_service,
        queue_service,
        pending_writes,
        shutdown_tx,
        started_at: Instant::now(),
        config: config.clone(),
    });

    if let Err(e) = run_startup_recovery(&state).await {
        tracing::warn!("Startup recovery failed: {}", e);
    }

    start_background_processor(state.clone());

    let router = create_router(state);
    let addr_str = format!("{host}:{port}");
    let addr: std::net::SocketAddr = addr_str.parse()?;

    let domain = if addr.is_ipv6() {
        socket2::Domain::IPV6
    } else {
        socket2::Domain::IPV4
    };

    let socket = socket2::Socket::new(domain, socket2::Type::STREAM, None)?;

    if let Err(e) = socket.set_reuse_address(true) {
        tracing::warn!("Failed to set SO_REUSEADDR: {}", e);
    }

    socket.bind(&addr.into()).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            anyhow::anyhow!(
                "Port {} is already in use.\n\n\
Another instance of opencode-mem is likely running.\n\
To stop it, run:\n\
  curl -X POST http://127.0.0.1:{}/api/admin/shutdown\n\
Or kill the process manually before starting a new server.",
                port,
                port
            )
        } else {
            anyhow::anyhow!("Failed to bind to {}: {}", addr_str, e)
        }
    })?;
    socket.listen(1024)?;

    let std_listener: std::net::TcpListener = socket.into();
    std_listener.set_nonblocking(true)?;

    let listener = tokio::net::TcpListener::from_std(std_listener)?;

    tracing::info!("Starting HTTP server on {}", addr_str);
    let is_restart = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let is_restart_clone = is_restart.clone();
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        is_restart_clone.store(
            shutdown_rx.recv().await.unwrap_or(false),
            std::sync::atomic::Ordering::Relaxed,
        );
    })
    .await?;

    tracing::info!("Waiting for background tasks to finish...");
    tracing::info!("Background tasks finished.");

    if is_restart.load(std::sync::atomic::Ordering::Relaxed) {
        std::process::exit(1);
    }

    Ok(())
}
