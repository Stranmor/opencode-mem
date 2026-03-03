# opencode-mem

High-performance persistent memory for AI coding agents. Built in Rust, backed by PostgreSQL.

![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white) ![PostgreSQL](https://img.shields.io/badge/PostgreSQL_16+-4169E1?logo=postgresql&logoColor=white) ![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg) ![MCP](https://img.shields.io/badge/MCP_Protocol-compatible-green)

## The Problem

AI coding agents lose all context between sessions. Every new conversation starts from zero, requiring the agent to re-learn the architecture, decisions, and constraints of your project. opencode-mem gives agents persistent, searchable memory that survives across sessions, projects, and machines.

## Why opencode-mem?

Most AI memory implementations rely on Node.js/TypeScript and split-brain architectures (SQLite for metadata + ChromaDB for vectors). opencode-mem takes a systems engineering approach:

* **Rust native** — Compiled binary, ~30MB RSS, no Node.js/Bun runtime overhead.
* **PostgreSQL + pgvector** — Single unified backend for relational, full-text, and vector data.
* **Infinite Memory** — Hierarchical summaries (5min → hour → day) with drill-down APIs. Raw events are never deleted.
* **Hybrid Search** — FTS BM25 (50%) + vector similarity (50%) via fastembed BGE-M3 (1024d, 100+ languages).
* **Enterprise Durability** — Dead letter queue, crash recovery, visibility timeout, streaming writes.
* **Privacy by Default** — `<private>` tag filtering at all entry points.
* **Context-Aware Dedup** — LLM decides CREATE/UPDATE/SKIP instead of blind duplication.
* **Comprehensive API** — 17 MCP tools and 65 HTTP endpoints.

## Features

### Memory Pipeline
End-to-end memory processing: observation ingestion → asynchronous queue → LLM compression → context-aware deduplication → storage. The pipeline intelligently evaluates incoming data against existing memory, issuing CREATE, UPDATE, or SKIP directives.

### Search
Advanced retrieval combining exact keyword matches with semantic understanding. Built on PostgreSQL full-text search (BM25) and pgvector cosine similarity, embedded locally via fastembed BGE-M3 (1024 dimensions, multilingual). Includes a dedicated knowledge base for skills, patterns, and gotchas.

### Infinite Memory
A dedicated backend for long-term AGI memory. Time-based hierarchical summaries continuously roll up (5-minute → hourly → daily), while preserving raw event data forever. Four specialized APIs enable semantic drill-down from abstract summaries back to exact raw events.

### Durability
Built for reliability with atomic PostgreSQL transactions. The ingestion pipeline features a robust pending queue with visibility timeouts, automatic crash recovery, and a dead letter queue for failed processing attempts.

### Privacy
Implicit safety via `<private>` content filtering applied across all ingestion paths (HTTP, MCP, CLI). Includes memory injection recursion prevention to stop infinite loops when AI agents read their own memory files.

### Web Viewer
Includes a dark-themed UI with Server-Sent Events (SSE) for real-time observation updates and system monitoring.

## Architecture

```text
                    ┌─────────────┐
                    │   OpenCode  │
                    │  IDE Plugin │
                    └──────┬──────┘
                           │ MCP (stdio) / HTTP
                    ┌──────▼──────┐
                    │  opencode-  │
                    │     mem     │
                    └──────┬──────┘
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
        ┌──────────┐ ┌──────────┐ ┌──────────┐
        │  Queue   │ │   LLM    │ │ Embedding│
        │Processor │ │Compress  │ │ BGE-M3   │
        └────┬─────┘ └────┬─────┘ └────┬─────┘
             │             │            │
             └─────────────┼────────────┘
                           ▼
                    ┌──────────────┐
                    │  PostgreSQL  │
                    │  + pgvector  │
                    └──────────────┘
```

### Project Structure

```text
crates/
├── core/              # Domain types, constants, error types
├── storage/           # PostgreSQL + pgvector + migrations
├── embeddings/        # fastembed BGE-M3 (1024d, multilingual)
├── search/            # Hybrid search (FTS BM25 + vector similarity)
├── llm/               # LLM compression & knowledge extraction
├── service/           # Business logic layer
├── http/              # Axum HTTP API (65 endpoints)
├── mcp/               # MCP server (stdio, 17 tools)
├── infinite-memory/   # Hierarchical time-based memory
└── cli/               # CLI binary
```

## Quick Start

### Prerequisites
* Rust 1.75+
* PostgreSQL 16+ with `pgvector` extension

### Build
```bash
cargo build -p opencode-mem-cli --release
```

### Database Setup
Ensure the `vector` extension is available in your database. Schema migrations will run automatically on the first start.
```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

### Configuration
Required environment variables:
```bash
export DATABASE_URL=postgres://user:pass@localhost/opencode_mem
```
Optional environment variables:
```bash
export ANTIGRAVITY_API_KEY=sk-your-key
export INFINITE_MEMORY_URL=postgres://user:pass@localhost/opencode_infinite
```

### Run
Start the HTTP server:
```bash
./target/release/opencode-mem-cli serve
```
Start in MCP mode (stdio):
```bash
./target/release/opencode-mem-cli mcp
```

### Systemd Service (User)
```ini
[Unit]
Description=OpenCode Memory Server
After=network.target postgresql.service

[Service]
Type=simple
ExecStart=/usr/local/bin/opencode-mem-cli serve
Environment=DATABASE_URL=postgres://user:pass@localhost/opencode_mem
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

## MCP Tools Reference

| Tool | Description |
|------|-------------|
| `__IMPORTANT` | 3-layer workflow guide: search → timeline → get_observations |
| `search` | Search memory by query, project, type, date range |
| `timeline` | Get chronological context within a time range |
| `get_observations` | Fetch full details for specific observation IDs |
| `memory_get` | Get observation by ID |
| `memory_recent` | Get recent observations |
| `memory_hybrid_search` | Combined FTS + keyword search |
| `memory_semantic_search` | Vector similarity search (BGE-M3) |
| `save_memory` | Store memory directly (bypasses LLM compression) |
| `knowledge_search` | Search knowledge base (skills, patterns, gotchas) |
| `knowledge_save` | Save knowledge entry |
| `knowledge_get` | Get knowledge entry by ID |
| `knowledge_list` | List knowledge entries by type |
| `knowledge_delete` | Delete knowledge entry |
| `infinite_expand` | Drill down from summary to child events |
| `infinite_time_range` | Query events by time range |
| `infinite_drill_hour` | Drill from day summary to hour summaries |
| `infinite_drill_minute` | Drill from hour summary to 5-minute summaries |

## HTTP API

The server exposes 65 endpoints organized across 11 handler modules: observations, sessions, search, knowledge, infinite memory, queue management, admin, and more. See `crates/http/src/handlers/` for the full API surface.

## Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | Yes | — | PostgreSQL connection string |
| `INFINITE_MEMORY_URL` | No | — | PostgreSQL for infinite memory (pgvector required) |
| `ANTIGRAVITY_API_KEY` | No | — | API key for LLM compression |
| `OPENCODE_MEM_API_URL` | No | — | LLM API endpoint |
| `OPENCODE_MEM_MODEL` | No | — | LLM model name |
| `OPENCODE_MEM_DISABLE_EMBEDDINGS` | No | `false` | Disable vector embeddings |
| `OPENCODE_MEM_EMBEDDING_THREADS` | No | `num_cpus - 1` | ONNX Runtime CPU threads |
| `OPENCODE_MEM_FILTER_PATTERNS` | No | — | Custom low-value filter patterns |
| `OPENCODE_MEM_EXCLUDED_PROJECTS` | No | — | Glob patterns for excluded projects |
| `OPENCODE_MEM_DEDUP_THRESHOLD` | No | `0.85` | Cosine similarity dedup threshold |
| `OPENCODE_MEM_INJECTION_DEDUP_THRESHOLD` | No | `0.80` | Memory injection recursion threshold |
| `OPENCODE_MEM_QUEUE_WORKERS` | No | `10` | Background queue concurrency |
| `OPENCODE_MEM_MAX_RETRY` | No | `3` | Queue retry limit |
| `OPENCODE_MEM_VISIBILITY_TIMEOUT` | No | `300` | Queue item lock period (seconds) |
| `OPENCODE_MEM_DLQ_TTL_DAYS` | No | `7` | Dead letter queue retention (days) |

## CLI Commands

```bash
opencode-mem-cli serve                # Start HTTP server
opencode-mem-cli mcp                  # Start MCP server (stdio)
opencode-mem-cli search <query>       # Search observations
opencode-mem-cli stats                # Show database statistics
opencode-mem-cli projects             # List tracked projects
opencode-mem-cli recent [n]           # Show recent observations
opencode-mem-cli get <id>             # Get observation by ID
opencode-mem-cli hook <subcommand>    # Run hooks (context, session-init, observe, summarize)
opencode-mem-cli backfill-embeddings  # Generate embeddings for existing observations
```

## Development

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all
```

## Acknowledgements

Inspired by [claude-mem](https://github.com/thedotmack/claude-mem) by thedotmack. opencode-mem is a ground-up Rust rewrite with PostgreSQL, pgvector, and significant architectural additions.

## License

MIT
