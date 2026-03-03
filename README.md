# opencode-mem
*Persistent, semantic memory server for AI coding agents.*

[![CI](https://img.shields.io/github/actions/workflow/status/Stranmor/opencode-mem/ci.yml?branch=main&style=flat-square)](https://github.com/Stranmor/opencode-mem/actions)
[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg?style=flat-square)](https://www.rust-lang.org)
[![Crates.io](https://img.shields.io/crates/v/opencode-mem-cli.svg?style=flat-square)](https://crates.io/crates/opencode-mem-cli)

Build autonomous AI coding agents that actually remember. `opencode-mem` is a purpose-built, type-safe Rust MCP (Model Context Protocol) server that gives your AI persistent memory. It combines blazing-fast full-text BM25 search with BGE-M3 1024d vector embeddings for semantic retrieval, backed by PostgreSQL and pgvector. Featuring a hierarchical infinite memory system, it enables AI agents to recall context across sessions, drill down from daily summaries to 5-minute event intervals, and maintain long-term project coherence—all from a single, zero-dependency binary.

## Why opencode-mem?

Unlike traditional TS/SQLite memory servers, `opencode-mem` is designed for unbounded scale and operational safety in autonomous AI coding environments.

| Feature | opencode-mem | Typical TS/SQLite |
|---------|-------------|-------------------|
| **Runtime** | Native binary (Rust) | Node.js/Bun |
| **Database** | PostgreSQL + pgvector | SQLite + ChromaDB |
| **Search** | Hybrid FTS BM25 + Vector | Separate engines |
| **Memory model** | Infinite (never deleted) | Fixed window / FIFO |
| **Crash recovery** | DLQ + visibility timeout | None |
| **Privacy** | Built-in `<private>` filtering | None |
| **Multilingual** | 100+ languages (BGE-M3) | English-centric |

## Advanced Capabilities

- 🧠 **Infinite Memory & Deep Zoom:** Solves the "static summaries" problem. Raw events are NEVER deleted. A hierarchical summarization pipeline (5min → hour → day) creates readable overviews, while the "Deep Zoom" drill-down API allows zooming from any high-level summary straight back to the original raw events.
- 🔍 **Semantic & Hybrid Search:** Powered by `fastembed-rs` using BGE-M3 (1024d, 100+ languages). Hybrid search combines FTS BM25 (50% weight) and vector similarity (50% weight) directly within PostgreSQL to yield unmatched precision without multiple network hops.
- 🧱 **Structured Metadata Extraction:** Summaries aren't just text. The LLM extracts specific `SummaryEntities` (files, functions, libraries, errors, decisions) via strict JSON schema enforcement, enabling fact-based search even when the human-readable summary is vague.
- 🛡️ **Context-Aware Compression:** The built-in AI agent analyzes the queue before creating new observations. It decides to either **CREATE**, **UPDATE**, or **SKIP**, effectively eliminating duplicate entries and deduplication overhead before they hit the database.
- 🔌 **17 MCP Tools** for seamless AI agent integration (search, fetch, analyze, drill-down).
- 🌐 **65+ HTTP API endpoints** for external integrations and dashboards.
- ⚡ **CLI with full hook system** (context injection, session init, observation, summarization).
- 🔒 **Privacy tags:** Built-in `<private>` content filtering applied across all ingest paths.
- 📦 **Single binary:** Zero runtime dependencies beyond PostgreSQL.

## Architecture

`opencode-mem` is designed as a robust workspace of specialized Rust crates to enforce modularity and prevent cyclic dependencies.

```mermaid
graph LR
    A[OpenCode IDE] -->|MCP/HTTP| B[opencode-mem]
    B --> C[Queue Processor]
    C --> D[LLM Compression]
    D --> E[PostgreSQL]
    E -->|pgvector| F[Semantic Search]
    E -->|tsvector/GIN| G[Full-Text Search]
    F & G --> H[Hybrid Results]
```

### Crate Structure
```text
crates/
├── core/            # Domain types (Observation, Session, etc.)
├── storage/         # PostgreSQL + pgvector + migrations
├── embeddings/      # Vector embeddings (fastembed BGE-M3, 1024d, multilingual)
├── search/          # Hybrid search (FTS + keyword + semantic)
├── llm/             # LLM compression (Antigravity API)
├── service/         # Business logic layer (ObservationService, SessionService, QueueService)
├── http/            # HTTP API (Axum)
├── mcp/             # MCP server (stdio)
├── infinite-memory/ # PostgreSQL + pgvector backend
└── cli/             # CLI binary
```

## Installation

You can install `opencode-mem-cli` globally using Cargo, or build it from source.

**Option 1: Install from crates.io**
```bash
cargo install opencode-mem-cli
```

**Option 2: Build from source**
```bash
git clone https://github.com/Stranmor/opencode-mem.git
cd opencode-mem
cargo build --release
# The binary will be available at target/release/opencode-mem-cli
```

## Quick Start

**Prerequisites:**
- Rust 1.75+
- PostgreSQL with `pgvector` extension

**1. Configure Database & LLM**
```bash
export DATABASE_URL="postgres://user:pass@host/dbname"
export OPENCODE_MEM_API_KEY="your-llm-api-key"
# Migrations will run automatically on the first start
```

**2. Run the Server**
```bash
# To run as an MCP server:
opencode-mem-cli mcp

# To run as an HTTP server:
opencode-mem-cli serve
```

**3. OpenCode Integration**
Add the following snippet to your `opencode.json` configuration file:

```json
{
  "mcpServers": {
    "memory": {
      "type": "stdio",
      "command": "/path/to/opencode-mem-cli",
      "args": ["mcp"],
      "env": {
        "DATABASE_URL": "postgres://user:pass@host/dbname",
        "OPENCODE_MEM_API_KEY": "your-api-key"
      }
    }
  }
}
```

## MCP Tools Reference

The server exposes 17 powerful MCP tools. The core workflow relies on the **3-Layer Workflow: Search → Timeline → Get Observations** to minimize token usage and maximize agent context efficiency.

| Tool | Description | Example Use Case |
|------|-------------|------------------|
| `search` | Search memory. Returns index with IDs. Semantic search with FTS fallback. | Finding all past observations about "authentication". |
| `timeline` | Get chronological context. Params: from, to, limit. | Seeing what work was done last Tuesday. |
| `get_observations` | Fetch full details for filtered IDs. Batch multiple IDs. | Retrieving the exact details of 3 specific bugs found in `search`. |
| `memory_get` | Get full observation details by ID. | Fetching a known observation record. |
| `memory_recent` | Get recent observations. | Reviewing what the agent did in the last hour. |
| `memory_hybrid_search` | Hybrid search combining FTS and keyword matching. | Exact keyword matching for specific variables or error codes. |
| `memory_semantic_search` | Smart search with semantic understanding, falls back to hybrid. | Finding conceptually related content (e.g., "auth" matches "login"). |
| `save_memory` | Save memory directly without LLM compression. | Manually injecting critical architectural decisions. |
| `knowledge_search` | Search global knowledge base for skills, patterns, gotchas. | Finding established patterns for API integrations in the project. |
| `knowledge_save` | Save new knowledge entry. | Recording a newly discovered workaround or "gotcha". |
| `knowledge_get` | Get knowledge entry by ID. | Retrieving specific instructions for a saved skill. |
| `knowledge_list` | List knowledge entries, optionally filtered by type. | Listing all known "gotchas" in the repository. |
| `knowledge_delete` | Delete knowledge entry by ID. | Removing outdated patterns. |
| `infinite_expand` | Expand a summary to see its child events. | Drilling down from a daily summary to see hourly details. |
| `infinite_time_range` | Get events within a time range. | Analyzing all raw events during a specific 2-hour debugging session. |
| `infinite_drill_hour` | Drill from day summary to hour summaries. | Viewing hourly breakdowns of a day's work. |
| `infinite_drill_minute`| Drill from hour summary to 5-minute summaries. | Deeply inspecting 5-minute event chunks for precise activity logs. |

## HTTP API

`opencode-mem` exposes 65+ HTTP endpoints organized across 13 core modules. This robust API surface allows for deep integration with custom dashboards, external agents, and CI/CD pipelines.

### Handler Modules
- **`observations`**: Core CRUD and bulk operations for agent observations.
- **`sessions`** & **`sessions_api`**: Session lifecycle management, summary generation, and retrieval.
- **`session_ops`**: Advanced session operations (merge, split, archive).
- **`infinite`**: Endpoints for Infinite Memory deep-zoom (`expand_summary`, `time_range`, `drill_hour`, `drill_minute`).
- **`search`**: Semantic, FTS, and hybrid search endpoints.
- **`knowledge`**: Global knowledge base management (skills, gotchas, patterns).
- **`queue`** & **`queue_processor`**: Pending observation queue management, DLQ (Dead Letter Queue) inspection.
- **`context`**: Context compilation and retrieval for agent injection.
- **`admin`**: Server health, configuration, and administrative overrides.
- **`api_docs`**: OpenAPI/Swagger documentation generation.
- **`mod`**: Module routing and middleware configuration.

## CLI Commands

The single binary `opencode-mem-cli` provides 10 powerful subcommands for both server operation and direct memory manipulation.

```bash
# Server Operations
opencode-mem-cli serve                 # Start the HTTP API server (default port 37777)
opencode-mem-cli mcp                   # Start the MCP stdio server for IDE integration

# Maintenance & Backfill
opencode-mem-cli backfill-embeddings   # Generate missing vector embeddings for past observations
opencode-mem-cli import-insights       # Import legacy JSON insights into the PostgreSQL DB

# Data Access & Search
opencode-mem-cli search <query>        # Search observations directly from the terminal
opencode-mem-cli get <id>              # Retrieve a specific observation by UUID
opencode-mem-cli recent                # Show the most recent memory events
opencode-mem-cli projects              # List all tracked projects and their observation counts
opencode-mem-cli stats                 # Show database statistics and queue health

# IDE Hooks (Internal use)
opencode-mem-cli hook context          # Retrieve formatted context for a new prompt
opencode-mem-cli hook session-init     # Initialize a new agent session
opencode-mem-cli hook observe          # Record a single observation
opencode-mem-cli hook summarize        # Trigger a session summarization
```

## Configuration

Configure the server via environment variables. `opencode-mem` is highly tunable for different deployment environments.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | **Yes** | - | PostgreSQL connection string |
| `OPENCODE_MEM_API_KEY` / `ANTIGRAVITY_API_KEY`| **Yes** | - | API key for LLM compression |
| `OPENCODE_MEM_API_URL` | No | `https://antigravity.quantumind.ru` | API base URL for the LLM provider |
| `OPENCODE_MEM_MODEL` | No | - | Model to use for compression (e.g., `gemini-3.1-pro-high`) |
| `OPENCODE_MEM_DISABLE_EMBEDDINGS` | No | `false` | Set to `1` or `true` to disable vector embeddings (CPU-only fallback) |
| `INFINITE_MEMORY_URL` / `OPENCODE_MEM_INFINITE_MEMORY`| No | (Uses `DATABASE_URL`) | Separate DB connection string for the infinite memory system |
| `OPENCODE_MEM_EXCLUDED_PROJECTS` | No | - | Glob patterns for projects to exclude (e.g., `*/tests/*`) |
| `OPENCODE_MEM_FILTER_PATTERNS` | No | - | Custom noise filter patterns (regex) |
| `OPENCODE_MEM_DEDUP_THRESHOLD` | No | `0.85` | Cosine similarity threshold for deduplication. Clamped `[0.0, 1.0]` |
| `OPENCODE_MEM_INJECTION_DEDUP_THRESHOLD`| No | `0.80` | Threshold for detecting IDE plugin injection loops. Clamped `[0.0, 1.0]` |
| `OPENCODE_MEM_EMBEDDING_THREADS` | No | `cores - 1` | Number of threads dedicated to ONNX embedding generation |
| `OPENCODE_MEM_MAX_RETRY` | No | `3` | Maximum retries for LLM compression |
| `OPENCODE_MEM_VISIBILITY_TIMEOUT` | No | `300s` | Visibility timeout for pending queue items |
| `OPENCODE_MEM_QUEUE_WORKERS` | No | `10` | Number of concurrent queue processing workers |
| `OPENCODE_MEM_DLQ_TTL_DAYS` | No | `7` | Days to keep messages in the Dead Letter Queue |
| `OPENCODE_MEM_MAX_CONTENT_CHARS` | No | `500` | Max characters per observation content field |
| `OPENCODE_MEM_MAX_TOTAL_CHARS` | No | `8000` | Max characters for combined LLM prompt |
| `OPENCODE_MEM_MAX_EVENTS` | No | `200` | Max raw events stored per infinite memory chunk |

## Development

### Running Tests
The project relies on a live PostgreSQL database for integration tests.

```bash
# Set up test database
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/opencode_mem_test"
sqlx database create
sqlx migrate run

# Run standard tests
cargo test

# Run LLM integration tests (ignored by default to prevent API spam)
cargo test -- --ignored
```

### Architecture Constraints
- **SPOT Axiom:** Single Point Of Truth. Data duplication is strictly forbidden.
- **Zero Fallback:** Missing data yields explicit `Error` or `None`, never fallback dummy data.
- **SQLx Compile-Time Checks:** All queries are validated against the live DB schema at compile time to prevent structural drift.
- **Modular Monolith:** Workspace segregation ensures clear domains between storage, logic, HTTP, and MCP interfaces.

## Project Status

This project maintains full feature parity with the upstream `claude-mem` (TypeScript) implementation, excluding IDE-specific hooks. Currently production-ready with experimental infinite memory features enabled by default.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request or open an issue on GitHub to discuss planned changes or improvements. When contributing, please ensure that all new features comply with the project's strict [SPOT Axiom] architectural guidelines.

## License

This project is licensed under the [MIT License](LICENSE).
