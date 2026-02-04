mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::hook::HookCommands;
use std::path::PathBuf;
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
    BackfillEmbeddings {
        #[arg(short, long, default_value = "100")]
        batch_size: usize,
    },
    ImportInsights {
        #[arg(short, long)]
        file: Option<String>,
        #[arg(short, long)]
        dir: Option<String>,
    },
    #[command(subcommand)]
    Hook(HookCommands),
}

pub fn get_db_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opencode-memory")
        .join("memory.db")
}

pub fn get_api_key() -> Result<String> {
    std::env::var("OPENCODE_MEM_API_KEY")
        .or_else(|_| std::env::var("ANTIGRAVITY_API_KEY"))
        .map_err(|_| {
            anyhow::anyhow!(
                "OPENCODE_MEM_API_KEY or ANTIGRAVITY_API_KEY environment variable must be set"
            )
        })
}

pub fn get_base_url() -> String {
    std::env::var("OPENCODE_MEM_API_URL")
        .unwrap_or_else(|_| "https://antigravity.quantumind.ru".to_string())
}

pub fn ensure_db_dir(db_path: &std::path::Path) -> Result<()> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { port, host } => {
            commands::serve::run(port, host).await?;
        }
        Commands::Mcp => {
            commands::mcp::run().await?;
        }
        Commands::Search {
            query,
            limit,
            project,
            obs_type,
        } => {
            commands::search::run_search(query, limit, project, obs_type)?;
        }
        Commands::Stats => {
            commands::search::run_stats()?;
        }
        Commands::Projects => {
            commands::search::run_projects()?;
        }
        Commands::Recent { limit } => {
            commands::search::run_recent(limit)?;
        }
        Commands::Get { id } => {
            commands::search::run_get(id)?;
        }
        Commands::BackfillEmbeddings { batch_size } => {
            commands::search::run_backfill_embeddings(batch_size)?;
        }
        Commands::ImportInsights { file, dir } => {
            commands::import_insights::run(file, dir)?;
        }
        Commands::Hook(hook_cmd) => {
            commands::hook::run(hook_cmd).await?;
        }
    }

    Ok(())
}
