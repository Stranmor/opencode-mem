# opencode-mem
*Persistent, semantic memory server for AI coding agents.*

[![CI](https://img.shields.io/github/actions/workflow/status/Stranmor/opencode-mem/ci.yml?branch=main&style=flat-square)](https://github.com/Stranmor/opencode-mem/actions)
[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg?style=flat-square)](https://www.rust-lang.org)
[![Crates.io](https://img.shields.io/crates/v/opencode-mem-cli.svg?style=flat-square)](https://crates.io/crates/opencode-mem-cli)

Build autonomous AI coding agents that actually remember. `opencode-mem` is a purpose-built, type-safe Rust MCP (Model Context Protocol) server that gives your AI persistent memory. It combines blazing-fast full-text BM25 search with BGE-M3 1024d vector embeddings for semantic retrieval, backed by PostgreSQL and pgvector. Featuring a hierarchical infinite memory system, it enables AI agents to recall context across sessions, drill down from daily summaries to 5-minute event intervals, and maintain long-term project coherence—all from a single, zero-dependency binary.

## Features

- 🧠 **Infinite Memory** with hierarchical summarization (5min → hour → day)
- 🔍 **Hybrid Search:** FTS BM25 + semantic vector similarity (fastembed BGE-M3, 1024d, 100+ languages)
- 🔌 **17 MCP Tools** for seamless AI agent integration
- 🌐 **65+ HTTP API endpoints**
- ⚡ **CLI with full hook system** (context injection, session init, observation, summarization)
- 🔒 **Privacy tags** (`<private>` content filtering)
- 📦 **Single binary**, zero runtime dependencies
- 🗄️ **PostgreSQL + pgvector backend**

## Architecture

```mermaid
graph TD
    A[CLI] --> E
    B[MCP Server] --> E
    C[HTTP API] --> E
    E[Service Layer<br>Observation, Session, Queue, Search]
    E --> F[Core Types]
    E --> G[Storage<br>PostgreSQL]
    E --> H[Embeddings<br>fastembed]
    E --> I[LLM<br>AI Compression]
    G --> J[Infinite Memory<br>Hierarchical Summarization]
    H --> J
```

## Quick Start

**Prerequisites:**
- Rust 1.75+
- PostgreSQL with `pgvector` extension

**1. Clone and Build**
```bash
git clone https://github.com/Stranmor/opencode-mem.git
cd opencode-mem
cargo build --release
```

**2. Configure Database**
```bash
export DATABASE_URL="postgres://user:pass@host/dbname"
# Migrations will run automatically on the first start
```

**3. Run the Server**
```bash
# To run as an MCP server:
opencode-mem-cli mcp

# To run as an HTTP server:
opencode-mem-cli serve
```

**4. OpenCode Integration**
Add the following snippet to your `opencode.json` configuration file:

```json
{
  "mcpServers": {
    "memory": {
      "type": "stdio",
      "command": "/path/to/opencode-mem-cli",
      "args": ["mcp"],
      "env": {
        "DATABASE_URL": "postgres://user:pass@host/dbname"
      }
    }
  }
}
```

## MCP Tools Reference

| Tool | Description |
|------|-------------|
| `search` | Search memory. Returns index with IDs. Semantic search with FTS fallback |
| `timeline` | Get chronological context within a time range |
| `get_observations` | Fetch full details for filtered observation IDs |
| `memory_get` | Get full observation details by ID |
| `memory_recent` | Get recent observations |
| `memory_hybrid_search` | Hybrid search combining FTS and keyword matching |
| `memory_semantic_search` | Semantic search with embedding similarity, FTS fallback |
| `save_memory` | Save memory directly without LLM compression |
| `knowledge_search` | Search global knowledge base for skills, patterns, gotchas |
| `knowledge_save` | Save new knowledge entry |
| `knowledge_get` | Get knowledge entry by ID |
| `knowledge_list` | List knowledge entries, optionally filtered by type |
| `knowledge_delete` | Delete knowledge entry by ID |
| `infinite_expand` | Expand a summary to see its child events |
| `infinite_time_range` | Get events within a time range |
| `infinite_drill_hour` | Drill down from day summary to hour summaries |
| `infinite_drill_minute` | Drill down from hour summary to 5-minute summaries |

## Configuration

| Variable | Required | Description |
|----------|----------|-------------|
| `DATABASE_URL` | Yes | PostgreSQL connection string |
| `OPENAI_API_KEY` | Yes | API key for LLM compression (OpenAI-compatible) |
| `OPENAI_BASE_URL` | No | Custom API base URL |
| `OPENAI_MODEL` | No | Model for compression (default: gpt-4o-mini) |
| `INFINITE_MEMORY_URL` | No | Separate DB for infinite memory (uses DATABASE_URL if unset) |
| `OPENCODE_MEM_DISABLE_EMBEDDINGS` | No | Set to `1` to disable vector embeddings |
| `OPENCODE_MEM_EXCLUDED_PROJECTS` | No | Glob patterns for projects to exclude |
| `OPENCODE_MEM_FILTER_PATTERNS` | No | Custom noise filter patterns |
| `OPENCODE_MEM_DEDUP_THRESHOLD` | No | Cosine similarity threshold for dedup (0.0-1.0) |

## CLI Commands

```bash
opencode-mem-cli serve     # Start HTTP server (default :37777)
opencode-mem-cli mcp       # Start MCP stdio server
opencode-mem-cli search    # Search observations
opencode-mem-cli stats     # Show database statistics
opencode-mem-cli projects  # List tracked projects
opencode-mem-cli recent    # Show recent observations
opencode-mem-cli get       # Get observation by ID
opencode-mem-cli hook      # IDE hook subcommands (context, session-init, observe, summarize)
```

## Project Status

This project maintains full feature parity with the upstream `claude-mem` (TypeScript) implementation, excluding IDE-specific hooks.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request or open an issue on GitHub to discuss planned changes or improvements.

## License

This project is licensed under the [MIT License](LICENSE).
