//! CLI for opencode-mem memory system.

#![allow(missing_docs, reason = "CLI binary with self-explanatory functions")]
#![allow(clippy::print_stdout, reason = "CLI output")]
#![allow(clippy::print_stderr, reason = "CLI error output")]
#![allow(clippy::absolute_paths, reason = "Explicit paths for clarity")]
#![allow(clippy::clone_on_ref_ptr, reason = "Arc cloning is intentional")]
#![allow(clippy::arithmetic_side_effects, reason = "Arithmetic is safe in context")]
#![allow(clippy::pattern_type_mismatch, reason = "Pattern matching style")]
#![allow(clippy::missing_errors_doc, reason = "CLI functions")]
#![allow(clippy::map_err_ignore, reason = "Error context is added")]
#![allow(clippy::unwrap_used, reason = "CLI panics are acceptable")]
#![allow(clippy::default_numeric_fallback, reason = "Numeric types are clear")]
#![allow(clippy::pub_with_shorthand, reason = "pub(crate) is clearer")]
#![allow(clippy::needless_pass_by_value, reason = "API design choice")]
#![allow(clippy::match_same_arms, reason = "Explicit arms are clearer")]
#![allow(clippy::unused_async, reason = "Async for consistency")]
#![allow(clippy::unnecessary_wraps, reason = "Result for consistency")]
#![allow(unused_results, reason = "Some results are intentionally ignored")]
#![allow(unused_crate_dependencies, reason = "Dependencies used in other modules")]
#![allow(clippy::pub_use, reason = "Re-exports are intentional")]
#![allow(clippy::redundant_pub_crate, reason = "pub(crate) is intentional for module visibility")]
#![allow(clippy::missing_docs_in_private_items, reason = "CLI binary")]
#![allow(clippy::implicit_return, reason = "Implicit return is idiomatic Rust")]
#![allow(clippy::question_mark_used, reason = "? operator is idiomatic Rust")]
#![allow(clippy::min_ident_chars, reason = "Short closure params are idiomatic")]
#![allow(clippy::missing_const_for_fn, reason = "Const fn not always beneficial")]
#![allow(clippy::shadow_reuse, reason = "Shadowing for unwrapping is idiomatic")]
#![allow(clippy::shadow_unrelated, reason = "Shadowing in different scopes is clear")]
#![allow(clippy::cognitive_complexity, reason = "CLI command handlers are inherently complex")]
#![allow(clippy::single_call_fn, reason = "CLI command functions are called once from main")]

mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::hook::HookCommands;
use opencode_mem_storage::StorageBackend;
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
    #[cfg(all(feature = "sqlite", feature = "postgres"))]
    Migrate,
}

#[must_use]
pub fn get_db_path() -> PathBuf {
    if let Ok(path) = std::env::var("OPENCODE_MEM_DB_PATH") {
        return PathBuf::from(path);
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opencode-memory")
        .join("memory.db")
}

pub fn get_api_key() -> Result<String> {
    std::env::var("OPENCODE_MEM_API_KEY").or_else(|_| std::env::var("ANTIGRAVITY_API_KEY")).map_err(
        |_| {
            anyhow::anyhow!(
                "OPENCODE_MEM_API_KEY or ANTIGRAVITY_API_KEY environment variable must be set"
            )
        },
    )
}

#[must_use]
pub fn get_base_url() -> String {
    std::env::var("OPENCODE_MEM_API_URL")
        .unwrap_or_else(|_| "https://antigravity.quantumind.ru".to_owned())
}

pub fn ensure_db_dir(db_path: &std::path::Path) -> Result<()> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

pub async fn create_storage() -> Result<StorageBackend> {
    #[cfg(feature = "postgres")]
    if let Ok(url) = std::env::var("DATABASE_URL") {
        tracing::info!("Using PostgreSQL backend");
        return StorageBackend::new_postgres(&url).await;
    }

    #[cfg(feature = "sqlite")]
    {
        let db_path = get_db_path();
        ensure_db_dir(&db_path)?;
        tracing::info!("Using SQLite backend: {}", db_path.display());
        return StorageBackend::new_sqlite(&db_path);
    }

    #[allow(unreachable_code)]
    Err(anyhow::anyhow!("No storage backend enabled. Enable 'sqlite' or 'postgres' feature."))
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
        },
        Commands::Mcp => {
            commands::mcp::run().await?;
        },
        Commands::Search { query, limit, project, obs_type } => {
            commands::search::run_search(query, limit, project, obs_type).await?;
        },
        Commands::Stats => {
            commands::search::run_stats().await?;
        },
        Commands::Projects => {
            commands::search::run_projects().await?;
        },
        Commands::Recent { limit } => {
            commands::search::run_recent(limit).await?;
        },
        Commands::Get { id } => {
            commands::search::run_get(id).await?;
        },
        Commands::BackfillEmbeddings { batch_size } => {
            commands::search::run_backfill_embeddings(batch_size).await?;
        },
        Commands::ImportInsights { file, dir } => {
            commands::import_insights::run(file, dir).await?;
        },
        Commands::Hook(hook_cmd) => {
            commands::hook::run(hook_cmd).await?;
        },
        #[cfg(all(feature = "sqlite", feature = "postgres"))]
        Commands::Migrate => {
            commands::migrate::run().await?;
        },
    }

    Ok(())
}
