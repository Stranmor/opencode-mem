# opencode-mem

*Persistent, semantic memory server for AI coding agents.*

[![CI](https://img.shields.io/github/actions/workflow/status/Stranmor/opencode-mem/ci.yml?branch=main&style=flat-square)](https://github.com/Stranmor/opencode-mem/actions)
[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg?style=flat-square)](https://www.rust-lang.org)

Build autonomous AI coding agents that actually remember. `opencode-mem` is a
purpose-built, type-safe Rust MCP server that gives your AI persistent memory.
It combines blazing-fast full-text BM25 search with BGE-M3 1024d vector
embeddings for semantic retrieval, backed by PostgreSQL and pgvector. Featuring
a hierarchical infinite memory system, it enables AI agents to recall context
across sessions, drill down from daily summaries to 5-minute event intervals,
and maintain long-term project coherence—all from a single, zero-dependency
binary.

## Why opencode-mem?

Unlike traditional TS/SQLite memory servers, `opencode-mem` is designed for
unbounded scale and operational safety in autonomous AI coding environments.

| Feature | opencode-mem | Typical TS/SQLite |
|---------|--------------|-------------------|
| **Runtime** | Native binary (Rust) | Node.js/Bun |
| **Database** | PostgreSQL + pgvector | SQLite + ChromaDB |
| **Search** | Hybrid FTS BM25 + Vector | Separate engines |
| **Memory model** | Infinite (never deleted) | Fixed window / FIFO |
| **Crash recovery**| DLQ + visibility timeout | None |
| **Privacy** | Built-in `<private>` filtering | None |
| **Multilingual** | 100+ languages (BGE-M3) | English-centric |

## Advanced Capabilities

- 🧠 **Infinite Memory & Deep Zoom:** Solves the "static summaries" problem.
- 🔍 **Semantic & Hybrid Search:** Powered by `fastembed-rs` using BGE-M3.
- 🧱 **Structured Metadata Extraction:** Summaries aren't just text.
- 🛡️ **Context-Aware Compression:** Eliminates duplicate entries and overhead.
- 🔌 **18 MCP Tools** for seamless AI agent integration.
- 🌐 **65+ HTTP API endpoints** for external integrations and dashboards.
- ⚡ **CLI with full hook system** for context injection and summarization.
- 🔒 **Privacy tags:** Built-in `<private>` content filtering.
- 📦 **Single binary:** Zero runtime dependencies beyond PostgreSQL.

## Architecture

`opencode-mem` is designed as a robust workspace of specialized Rust crates to
enforce modularity and prevent cyclic dependencies.

```mermaid
graph TD
    A[OpenCode IDE] --> B[opencode-mem]
    B --> C[Queue Processor]
    C --> D[LLM Compression]
    D --> E[PostgreSQL]
    E --> F[Semantic Search]
    E --> G[Full-Text Search]
    F --> H[Hybrid Results]
    G --> H
```

### Crate Structure

```text
crates/
├── core/            # Domain types (Observation, Session, etc.)
├── storage/         # PostgreSQL + pgvector + migrations
├── embeddings/      # Vector embeddings (fastembed BGE-M3, 1024d)
├── search/          # Hybrid search (FTS + keyword + semantic)
├── llm/             # LLM compression (Antigravity API)
├── service/         # Business logic layer
├── http/            # HTTP API (Axum)
├── mcp/             # MCP server (stdio)
├── infinite-memory/ # PostgreSQL + pgvector backend
└── cli/             # CLI binary
```

## Installation

You can install `opencode-mem-cli` globally using Cargo, or build it from
source.

**Build from source**

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

### 1. Configure Database & LLM

```bash
# Set your PostgreSQL URL and API key
export DATABASE_URL="postgres://user:pass@host:5432/db"
export OPENCODE_MEM_API_KEY="your-llm-api-key"
```

### 2. Run the Server

```bash
# To run as an MCP server:
opencode-mem-cli mcp

# To run as an HTTP server:
opencode-mem-cli serve
```

### 3. OpenCode Integration

Add the following snippet to your `opencode.json` configuration file:

```json
{
  "mcpServers": {
    "memory": {
      "type": "stdio",
      "command": "opencode-mem-cli",
      "args": ["mcp"],
      "env": {
        "DATABASE_URL": "postgres://user:pass@host:5432/db",
        "OPENCODE_MEM_API_KEY": "your-key"
      }
    }
  }
}
```

## MCP Tools Reference

The server exposes 18 powerful MCP tools.

| Tool | Description |
|------|-------------|
| `search` | Semantic search with FTS fallback. |
| `timeline` | Get chronological context (from, to, limit). |
| `get_observations` | Fetch full details for filtered IDs. |
| `memory_get` | Get full observation details by ID. |
| `memory_recent` | Get recent observations. |
| `memory_hybrid_search` | Hybrid search combining FTS and keyword matching. |
| `memory_semantic_search` | Smart search with semantic understanding. |
| `save_memory` | Save memory directly without LLM compression. |
| `knowledge_search` | Search global knowledge base for patterns. |
| `knowledge_save` | Save new knowledge entry. |
| `knowledge_get` | Get knowledge entry by ID. |
| `knowledge_list` | List knowledge entries by type. |
| `knowledge_delete` | Delete knowledge entry by ID. |
| `infinite_expand` | Expand a summary to see its child events. |
| `infinite_time_range` | Get events within a time range. |
| `infinite_drill_hour` | Drill from day summary to hour summaries. |
| `infinite_drill_minute`| Drill from hour summary to 5-minute summaries. |

## HTTP API

`opencode-mem` exposes 65+ HTTP endpoints organized across 13 core modules.

## CLI Commands

The single binary `opencode-mem-cli` provides 10 powerful subcommands.

```bash
# Server Operations
opencode-mem-cli serve                 # Start the HTTP API server
opencode-mem-cli mcp                   # Start the MCP stdio server

# Data Access & Search
opencode-mem-cli search <query>        # Search observations
opencode-mem-cli get <id>              # Retrieve a specific observation
opencode-mem-cli recent                # Show the most recent memory events
opencode-mem-cli projects              # List all tracked projects
opencode-mem-cli stats                 # Show database statistics
```

## Configuration

Configure the server via environment variables.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | **Yes** | - | Postgres connection string |
| `OPENCODE_MEM_API_KEY` | **Yes** | - | API key for LLM compression |
| `OPENCODE_MEM_API_URL` | No | `https://antigr...` | API base URL |
| `OPENCODE_MEM_MODEL` | No | - | Model for compression |
| `OPENCODE_MEM_DISABLE_EMBEDDINGS` | No | `false` | Disable embeddings |
| `INFINITE_MEMORY_URL` | No | `DATABASE_URL` | DB connection for infinite memory |
| `OPENCODE_MEM_EXCLUDED_PROJECTS` | No | - | Glob patterns for project exclusions |
| `OPENCODE_MEM_FILTER_PATTERNS` | No | - | Custom noise filter regex patterns |
| `OPENCODE_MEM_DEDUP_THRESHOLD` | No | `0.85` | Cosine similarity threshold |
| `OPENCODE_MEM_INJECTION_DEDUP_THRESHOLD`| No | `0.80` | Plugin loop prevention threshold |
| `OPENCODE_MEM_EMBEDDING_THREADS` | No | `cores - 1` | ONNX generation worker threads |
| `OPENCODE_MEM_MAX_RETRY` | No | `3` | Max LLM compression retry attempts |
| `OPENCODE_MEM_VISIBILITY_TIMEOUT` | No | `300s` | Pending message visibility timeout |
| `OPENCODE_MEM_QUEUE_WORKERS` | No | `10` | Concurrent queue background workers |
| `OPENCODE_MEM_DLQ_TTL_DAYS` | No | `7` | Days to retain items in dead letter queue |
| `OPENCODE_MEM_MAX_CONTENT_CHARS` | No | `500` | Max chars per extracted content field |
| `OPENCODE_MEM_MAX_TOTAL_CHARS` | No | `8000` | Max characters for LLM prompt payload |
| `OPENCODE_MEM_MAX_EVENTS` | No | `200` | Max raw events per infinite memory chunk |

## Development

```bash
# Test DB setup
export DATABASE_URL="postgres://user:pass@localhost:5432/test"
sqlx database create
sqlx migrate run

# Tests
cargo test
cargo test -- --ignored
```

## License

This project is licensed under the [MIT License](LICENSE).
