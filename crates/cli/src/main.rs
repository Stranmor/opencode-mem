use anyhow::Result;
use clap::{Parser, Subcommand};
use opencode_mem_http::{create_router, AppState, Settings};
use opencode_mem_llm::LlmClient;
use opencode_mem_mcp::run_mcp_server;
use opencode_mem_storage::Storage;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::{broadcast, Semaphore, RwLock};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "opencode-mem")]
#[command(about = "Persistent memory system for OpenCode", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Serve {
        #[arg(short, long, default_value = "37777")]
        port: u16,
        #[arg(short = 'H', long, default_value = "127.0.0.1")]
        host: String,
    },
    Mcp,
    Search {
        query: String,
        #[arg(short, long, default_value = "20")]
        limit: usize,
        #[arg(short, long)]
        project: Option<String>,
        #[arg(short = 't', long)]
        obs_type: Option<String>,
    },
    Stats,
    Projects,
    Recent {
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    Get {
        id: String,
    },
}

fn get_db_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opencode-memory")
        .join("memory.db")
}

fn get_api_key() -> Result<String> {
    std::env::var("OPENCODE_MEM_API_KEY")
        .or_else(|_| std::env::var("ANTIGRAVITY_API_KEY"))
        .map_err(|_| anyhow::anyhow!("OPENCODE_MEM_API_KEY or ANTIGRAVITY_API_KEY environment variable must be set"))
}

fn get_base_url() -> String {
    std::env::var("OPENCODE_MEM_API_URL")
        .unwrap_or_else(|_| "https://antigravity.quantumind.ru".to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let cli = Cli::parse();
    let db_path = get_db_path();
    
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let storage = Storage::new(&db_path)?;

    match cli.command {
        Commands::Serve { port, host } => {
            let llm = LlmClient::new(get_api_key()?, get_base_url());
            let (event_tx, _) = broadcast::channel(100);
            let state = Arc::new(AppState {
                storage,
                llm,
                semaphore: Arc::new(Semaphore::new(10)),
                event_tx,
                processing_active: AtomicBool::new(true),
                settings: RwLock::new(Settings::default()),
            });
            let router = create_router(state);
            let addr = format!("{}:{}", host, port);
            tracing::info!("Starting HTTP server on {}", addr);
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            axum::serve(listener, router).await?;
        }
        Commands::Mcp => {
            tokio::task::spawn_blocking(move || {
                run_mcp_server(Arc::new(storage));
            })
            .await?;
        }
        Commands::Search { query, limit, project, obs_type } => {
            let results = storage.search_with_filters(
                Some(&query),
                project.as_deref(),
                obs_type.as_deref(),
                limit,
            )?;
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
        Commands::Stats => {
            let stats = storage.get_stats()?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        Commands::Projects => {
            let projects = storage.get_all_projects()?;
            println!("{}", serde_json::to_string_pretty(&projects)?);
        }
        Commands::Recent { limit } => {
            let results = storage.get_recent(limit)?;
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
        Commands::Get { id } => {
            match storage.get_by_id(&id)? {
                Some(obs) => println!("{}", serde_json::to_string_pretty(&obs)?),
                None => println!("Observation not found: {}", id),
            }
        }
    }

    Ok(())
}
