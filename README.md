<div align="center">

# opencode-mem

**Persistent, semantic memory for AI coding agents.**

Give your AI agents long-term memory that actually works — hybrid search, hierarchical summaries, and infinite recall, all from a single Rust binary.

[![CI](https://img.shields.io/github/actions/workflow/status/Stranmor/opencode-mem/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/Stranmor/opencode-mem/actions)
[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg?style=flat-square)](https://www.rust-lang.org)
[![GitHub Stars](https://img.shields.io/github/stars/Stranmor/opencode-mem?style=flat-square)](https://github.com/Stranmor/opencode-mem/stargazers)
[![GitHub Issues](https://img.shields.io/github/issues/Stranmor/opencode-mem?style=flat-square)](https://github.com/Stranmor/opencode-mem/issues)

</div>

---

`opencode-mem` is a type-safe Rust [MCP](https://modelcontextprotocol.io/) (Model Context Protocol) server that gives AI coding agents persistent memory. It combines full-text BM25 search with BGE-M3 1024-dimensional vector embeddings for semantic retrieval, backed by PostgreSQL and pgvector. A hierarchical infinite memory system lets agents recall context across sessions, drill down from daily summaries to 5-minute event intervals, and maintain long-term project coherence.

A Rust port of [claude-mem](https://github.com/thedotmack/claude-mem) with full feature parity and additional capabilities.

## Why opencode-mem?

| Feature | opencode-mem | Typical TS/SQLite |
|---------|-------------|-------------------|
| **Runtime** | Native binary (Rust) | Node.js / Bun |
| **Database** | PostgreSQL + pgvector | SQLite + ChromaDB |
| **Search** | Hybrid FTS BM25 + Vector | Separate engines |
| **Memory model** | Infinite (never deleted) | Fixed window / FIFO |
| **Crash recovery** | DLQ + visibility timeout | None |
| **Privacy** | Built-in `<private>` filtering | None |
| **Multilingual** | 100+ languages (BGE-M3) | English-centric |

## Key Features

- **Infinite Memory & Deep Zoom** — Raw events are never deleted. A hierarchical summarization pipeline (5min → hour → day) creates readable overviews, while the drill-down API lets you zoom from any summary back to raw events.
- **Hybrid Search** — Combines FTS BM25 (50%) and vector similarity (50%) directly within PostgreSQL. Powered by `fastembed-rs` using BGE-M3 (1024d, 100+ languages).
- **Structured Metadata Extraction** — The LLM extracts `SummaryEntities` (files, functions, libraries, errors, decisions) via strict JSON schema, enabling fact-based search even when text summaries are vague.
- **Context-Aware Compression** — The AI agent analyzes existing observations before creating new ones. It decides to **CREATE**, **UPDATE**, or **SKIP**, eliminating duplicates before they hit the database.
- **18 MCP Tools** for seamless AI agent integration (search, fetch, analyze, drill-down).
- **65+ HTTP API endpoints** for external integrations and dashboards.
- **CLI with full hook system** (context injection, session init, observation, summarization).
- **Privacy tags** — Built-in `<private>` content filtering across all ingest paths.
- **Circuit breaker** — Graceful degradation when PostgreSQL is unavailable, with automatic recovery on reconnect.
- **Single binary** — Zero runtime dependencies beyond PostgreSQL.

## Architecture

```mermaid
graph LR
    A[AI Agent / IDE] -->|MCP stdio| B[opencode-mem]
    A -->|HTTP :37777| B
    B --> C[Queue Processor]
    C --> D[LLM Compression]
    D --> E[(PostgreSQL)]
    E -->|pgvector| F[Semantic Search]
    E -->|tsvector / GIN| G[Full-Text Search]
    F & G --> H[Hybrid Results]
```

### Crate Structure

```text
crates/
├── core/              # Domain types (Observation, Session, Knowledge, etc.)
├── storage/           # PostgreSQL + pgvector + migrations + circuit breaker
├── embeddings/        # Vector embeddings (fastembed BGE-M3, 1024d, multilingual)
├── search/            # Hybrid search (FTS + keyword + semantic)
├── llm/               # LLM compression (OpenAI-compatible API)
├── service/           # Business logic (ObservationService, SessionService, QueueService)
├── http/              # HTTP API (Axum)
├── mcp/               # MCP server (stdio)
├── infinite-memory/   # Hierarchical infinite memory backend
└── cli/               # CLI binary
```

## Installation

### From source

```bash
git clone https://github.com/Stranmor/opencode-mem.git
cd opencode-mem
cargo build --release
# Binary: target/release/opencode-mem-cli
```

## Quick Start

**Prerequisites:** Rust 1.88+ · PostgreSQL with [`pgvector`](https://github.com/pgvector/pgvector) extension

### 1. Configure

```bash
export DATABASE_URL="postgres://user:pass@localhost/opencode_mem"
export OPENCODE_MEM_API_KEY="your-openai-compatible-api-key"
export OPENCODE_MEM_API_URL="https://api.openai.com"  # or any compatible endpoint
```

Migrations run automatically on first start.

### 2. Run

```bash
# MCP server (for IDE integration):
opencode-mem-cli mcp

# HTTP server (for dashboards and external integrations):
opencode-mem-cli serve
```

### 3. Integrate with OpenCode

Add to your `opencode.json`:

```json
{
  "mcpServers": {
    "memory": {
      "type": "stdio",
      "command": "/path/to/opencode-mem-cli",
      "args": ["mcp"],
      "env": {
        "DATABASE_URL": "postgres://user:pass@localhost/opencode_mem",
        "OPENCODE_MEM_API_KEY": "your-api-key",
        "OPENCODE_MEM_API_URL": "https://api.openai.com"
      }
    }
  }
}
```

## MCP Tools

The server exposes 18 MCP tools. The recommended workflow is the **3-Layer Pattern** — Search → Timeline → Get Observations — to minimize token usage.

| Tool | Description |
|------|-------------|
| `search` | Search memory with semantic understanding. Returns index with IDs. |
| `timeline` | Get chronological context within a time range. |
| `get_observations` | Fetch full details for specific observation IDs. |
| `memory_get` | Get a single observation by ID. |
| `memory_recent` | Get the most recent observations. |
| `memory_hybrid_search` | Combined FTS + keyword search. |
| `memory_semantic_search` | Pure semantic search with hybrid fallback. |
| `save_memory` | Save memory directly (bypasses LLM compression). |
| `knowledge_search` | Search the global knowledge base. |
| `knowledge_save` | Save a new knowledge entry (skill, pattern, gotcha). |
| `knowledge_get` | Get a knowledge entry by ID. |
| `knowledge_list` | List knowledge entries by type. |
| `knowledge_delete` | Delete a knowledge entry. |
| `infinite_expand` | Expand a summary to see child events. |
| `infinite_time_range` | Get events within a time range. |
| `infinite_drill_hour` | Drill from day summary to hour summaries. |
| `infinite_drill_minute` | Drill from hour summary to 5-minute summaries. |
| `__IMPORTANT` | Workflow documentation (3-Layer Pattern). |

## HTTP API

65+ endpoints organized across 11 handler modules:

- **`observations`** — CRUD and bulk operations for observations
- **`sessions`** / **`sessions_api`** — Session lifecycle, summaries, retrieval
- **`session_ops`** — Advanced operations (merge, split, archive)
- **`infinite`** — Deep-zoom endpoints (`expand_summary`, `time_range`, `drill_hour`, `drill_minute`)
- **`search`** — Semantic, FTS, and hybrid search
- **`knowledge`** — Global knowledge base management
- **`queue`** — Pending queue and DLQ inspection
- **`context`** — Context compilation for agent injection
- **`admin`** — Health checks, configuration, diagnostics

## CLI

```bash
# Server
opencode-mem-cli serve                 # HTTP API server (port 37777)
opencode-mem-cli mcp                   # MCP stdio server

# Maintenance
opencode-mem-cli backfill-embeddings   # Generate missing vector embeddings
opencode-mem-cli import-insights       # Import legacy JSON insights

# Data Access
opencode-mem-cli search <query>        # Search observations
opencode-mem-cli get <id>              # Get observation by UUID
opencode-mem-cli recent                # Recent observations
opencode-mem-cli projects              # List tracked projects
opencode-mem-cli stats                 # Database statistics and queue health

# IDE Hooks
opencode-mem-cli hook context          # Retrieve context for prompt injection
opencode-mem-cli hook session-init     # Initialize a new session
opencode-mem-cli hook observe          # Record an observation
opencode-mem-cli hook summarize        # Trigger session summarization
```

## Configuration

All configuration is via environment variables:

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | **Yes** | — | PostgreSQL connection string |
| `OPENCODE_MEM_API_KEY` | **Yes** | — | API key for the LLM provider |
| `OPENCODE_MEM_API_URL` | No | `https://api.openai.com` | OpenAI-compatible API base URL |
| `OPENCODE_MEM_MODEL` | No | — | Model for compression (e.g., `gpt-4o`) |
| `OPENCODE_MEM_DISABLE_EMBEDDINGS` | No | `false` | Disable vector embeddings (`1` or `true`) |
| `INFINITE_MEMORY_URL` | No | `DATABASE_URL` | Separate DB for infinite memory |
| `OPENCODE_MEM_EXCLUDED_PROJECTS` | No | — | Glob patterns for excluded projects |
| `OPENCODE_MEM_FILTER_PATTERNS` | No | — | Custom noise filter patterns (regex) |
| `OPENCODE_MEM_DEDUP_THRESHOLD` | No | `0.85` | Cosine similarity for dedup `[0.0, 1.0]` |
| `OPENCODE_MEM_INJECTION_DEDUP_THRESHOLD` | No | `0.80` | IDE injection loop detection `[0.0, 1.0]` |
| `OPENCODE_MEM_EMBEDDING_THREADS` | No | `cores - 1` | ONNX embedding threads |
| `OPENCODE_MEM_MAX_RETRY` | No | `3` | LLM compression retries |
| `OPENCODE_MEM_VISIBILITY_TIMEOUT` | No | `300s` | Queue visibility timeout |
| `OPENCODE_MEM_QUEUE_WORKERS` | No | `10` | Concurrent queue workers |
| `OPENCODE_MEM_DLQ_TTL_DAYS` | No | `7` | Dead letter queue retention |
| `OPENCODE_MEM_MAX_CONTENT_CHARS` | No | `500` | Max chars per observation field |
| `OPENCODE_MEM_MAX_TOTAL_CHARS` | No | `8000` | Max chars for LLM prompt |
| `OPENCODE_MEM_MAX_EVENTS` | No | `200` | Max raw events per memory chunk |

## Development

### Prerequisites

- Rust 1.88+
- PostgreSQL with `pgvector` extension
- An OpenAI-compatible LLM API (for compression features)

### Running Tests

```bash
export DATABASE_URL="postgres://postgres:postgres@localhost:5432/opencode_mem_test"

# Unit tests (no DB required)
cargo test --workspace

# Integration tests (requires running PostgreSQL)
cargo test --workspace -- --ignored
```

### Code Quality

```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
```

### Architecture Principles

- **SPOT** — Single Point of Truth. No data duplication.
- **Zero Fallback** — Missing data returns `Error` or `None`, never dummy values.
- **Compile-time query validation** — SQLx verifies all queries against the live DB schema.
- **Modular workspace** — 10 crates with enforced domain boundaries.

## Project Status

Production-ready. Full feature parity with the upstream [claude-mem](https://github.com/thedotmack/claude-mem) TypeScript implementation (excluding IDE-specific hooks). Infinite memory and semantic search are enabled by default.

## Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

[MIT](LICENSE)
