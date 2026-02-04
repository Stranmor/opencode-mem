use anyhow::Result;
use clap::{Parser, Subcommand};
use opencode_mem_core::{ObservationHookRequest, SessionInitHookRequest, SummarizeHookRequest};
use opencode_mem_http::{create_router, AppState, Settings};
use opencode_mem_infinite::InfiniteMemory;
use opencode_mem_llm::LlmClient;
use opencode_mem_mcp::run_mcp_server;
use opencode_mem_storage::Storage;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock, Semaphore};
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
    #[command(subcommand)]
    Hook(HookCommands),
}

#[derive(Subcommand)]
enum HookCommands {
    Context {
        #[arg(short, long)]
        project: Option<String>,
        #[arg(short, long, default_value = "50")]
        limit: usize,
        #[arg(long, default_value = "http://127.0.0.1:37777")]
        endpoint: String,
    },
    SessionInit {
        #[arg(long)]
        content_session_id: Option<String>,
        #[arg(short, long)]
        project: Option<String>,
        #[arg(long)]
        user_prompt: Option<String>,
        #[arg(long, default_value = "http://127.0.0.1:37777")]
        endpoint: String,
    },
    Observe {
        #[arg(short, long)]
        tool: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(short, long)]
        project: Option<String>,
        #[arg(long, default_value = "http://127.0.0.1:37777")]
        endpoint: String,
    },
    Summarize {
        #[arg(long)]
        content_session_id: Option<String>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long, default_value = "http://127.0.0.1:37777")]
        endpoint: String,
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
        .map_err(|_| {
            anyhow::anyhow!(
                "OPENCODE_MEM_API_KEY or ANTIGRAVITY_API_KEY environment variable must be set"
            )
        })
}

fn get_base_url() -> String {
    std::env::var("OPENCODE_MEM_API_URL")
        .unwrap_or_else(|_| "https://antigravity.quantumind.ru".to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let db_path = get_db_path();

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let storage = Storage::new(&db_path)?;

    match cli.command {
        Commands::Serve { port, host } => {
            let api_key = get_api_key()?;
            let llm = LlmClient::new(api_key.clone(), get_base_url());
            let (event_tx, _) = broadcast::channel(100);
            
            // Initialize infinite memory (PostgreSQL + pgvector)
            let infinite_mem = match std::env::var("INFINITE_MEMORY_URL") {
                Ok(url) => {
                    match InfiniteMemory::new(&url, &api_key).await {
                        Ok(mem) => {
                            tracing::info!("Connected to infinite memory: {}", url);
                            Some(Arc::new(mem))
                        }
                        Err(e) => {
                            tracing::warn!("Failed to connect to infinite memory: {}", e);
                            None
                        }
                    }
                }
                Err(_) => {
                    tracing::info!("INFINITE_MEMORY_URL not set, infinite memory disabled");
                    None
                }
            };
            
            let state = Arc::new(AppState {
                storage,
                llm,
                semaphore: Arc::new(Semaphore::new(10)),
                event_tx,
                processing_active: AtomicBool::new(true),
                settings: RwLock::new(Settings::default()),
                infinite_mem,
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
        Commands::Search {
            query,
            limit,
            project,
            obs_type,
        } => {
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
        Commands::Get { id } => match storage.get_by_id(&id)? {
            Some(obs) => println!("{}", serde_json::to_string_pretty(&obs)?),
            None => println!("Observation not found: {}", id),
        },
        Commands::Hook(hook_cmd) => {
            drop(storage);
            handle_hook_command(hook_cmd).await?;
        }
    }

    Ok(())
}

fn get_project_from_stdin() -> Option<String> {
    if atty::is(atty::Stream::Stdin) {
        return None;
    }
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok()?;
    let json: serde_json::Value = serde_json::from_str(&input).ok()?;
    json.get("project")
        .or_else(|| json.get("project_path"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

fn build_session_init_request(
    content_session_id: Option<String>,
    project: Option<String>,
    user_prompt: Option<String>,
) -> Result<SessionInitHookRequest> {
    let session_id = content_session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    Ok(SessionInitHookRequest {
        content_session_id: session_id,
        project,
        user_prompt,
    })
}

fn build_observation_request(
    tool: Option<String>,
    session_id: Option<String>,
    project: Option<String>,
) -> Result<ObservationHookRequest> {
    let mut input_str = String::new();
    if !atty::is(atty::Stream::Stdin) {
        std::io::stdin().read_to_string(&mut input_str)?;
    }
    let tool_name = tool.unwrap_or_else(|| "unknown".to_string());
    let input_json: Option<serde_json::Value> = if input_str.is_empty() {
        None
    } else {
        serde_json::from_str(&input_str).ok()
    };
    let output = if input_str.is_empty() {
        "".to_string()
    } else {
        input_str
    };
    Ok(ObservationHookRequest {
        tool: tool_name,
        session_id,
        call_id: None,
        project,
        input: input_json,
        output,
    })
}

fn build_summarize_request(
    content_session_id: Option<String>,
    session_id: Option<String>,
) -> Result<SummarizeHookRequest> {
    Ok(SummarizeHookRequest {
        content_session_id,
        session_id,
    })
}

async fn handle_hook_command(cmd: HookCommands) -> Result<()> {
    let client = reqwest::Client::new();

    match cmd {
        HookCommands::Context {
            project,
            limit,
            endpoint,
        } => {
            let project = project.or_else(get_project_from_stdin).ok_or_else(|| {
                anyhow::anyhow!("Project required: use --project or pipe JSON with 'project' field")
            })?;
            let url = format!("{}/context/inject", endpoint);
            let resp = client
                .get(&url)
                .query(&[("project", &project), ("limit", &limit.to_string())])
                .send()
                .await?;
            let body: serde_json::Value = resp.json().await?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        HookCommands::SessionInit {
            content_session_id,
            project,
            user_prompt,
            endpoint,
        } => {
            let req = build_session_init_request(content_session_id, project, user_prompt)?;
            let url = format!("{}/api/sessions/init", endpoint);
            let resp = client.post(&url).json(&req).send().await?;
            let body: serde_json::Value = resp.json().await?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        HookCommands::Observe {
            tool,
            session_id,
            project,
            endpoint,
        } => {
            let req = build_observation_request(tool, session_id, project)?;
            let url = format!("{}/observe", endpoint);
            let resp = client.post(&url).json(&req).send().await?;
            let body: serde_json::Value = resp.json().await?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        HookCommands::Summarize {
            content_session_id,
            session_id,
            endpoint,
        } => {
            let req = build_summarize_request(content_session_id, session_id)?;
            let url = format!("{}/api/sessions/summarize", endpoint);
            let resp = client.post(&url).json(&req).send().await?;
            let body: serde_json::Value = resp.json().await?;
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
    }

    Ok(())
}

