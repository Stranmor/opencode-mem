# opencode-mem

Rust port of [claude-mem](https://github.com/thedotmack/claude-mem) for OpenCode. Persistent cross-session memory for AI agents via MCP protocol.

17 MCP tools, 65+ HTTP endpoints, hybrid search (FTS + vector similarity).

## Architecture

PostgreSQL with tsvector + GIN for full-text search and pgvector for vector similarity.

```
crates/
  core/              Domain types (Observation, Session, etc.)
  storage/           Database layer — PostgreSQL + pgvector
  embeddings/        Vector embeddings (fastembed BGE-M3, 1024d, multilingual)
  search/            Hybrid search (FTS BM25 50% + vector similarity 50%)
  llm/               LLM compression (Antigravity API)
  service/           Business logic (ObservationService, SessionService)
  http/              HTTP API (Axum, 65+ endpoints)
  mcp/               MCP server (stdio, 17 tools)
  infinite-memory/   PostgreSQL + pgvector backend for long-term memory
  cli/               CLI binary
```

## Build

```bash
cargo build -p opencode-mem-cli --release
```

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `DATABASE_URL` | **Yes** | PostgreSQL connection string. Example: `postgres://user:pass@localhost/opencode_mem` |
| `INFINITE_MEMORY_URL` | No | PostgreSQL connection for infinite memory backend (pgvector required) |
| `ANTIGRAVITY_API_KEY` | No | API key for LLM compression (observation summarization) |
| `OPENCODE_MEM_FILTER_PATTERNS` | No | Custom low-value observation filter patterns |
| `OPENCODE_MEM_EXCLUDED_PROJECTS` | No | Glob patterns for excluded projects (comma-separated, `~` expansion) |

PostgreSQL requires the `pgvector` extension for semantic search:

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

## CLI Commands

```
opencode-mem serve                Start HTTP server (default: port 37777)
opencode-mem mcp                  Start MCP server (stdio)
opencode-mem search <query>       Search observations
opencode-mem stats                Show database statistics
opencode-mem projects             List tracked projects
opencode-mem recent [n]           Show recent observations
opencode-mem get <id>             Get observation by ID
opencode-mem hook <subcommand>    Run hooks (context, session-init, observe, summarize)
opencode-mem backfill-embeddings  Generate embeddings for existing observations
```

### Backfill Embeddings

Generate vector embeddings for observations that predate semantic search:

```bash
opencode-mem backfill-embeddings --batch-size 100
```

## Systemd Service

```ini
[Unit]
Description=OpenCode Memory Server
After=network.target postgresql.service

[Service]
Type=simple
ExecStart=/usr/local/bin/opencode-mem serve
Environment=DATABASE_URL=postgres://user:pass@localhost/opencode_mem
Environment=ANTIGRAVITY_API_KEY=sk-your-key
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

Install as user service:

```bash
cp opencode-memory.service ~/.config/systemd/user/
systemctl --user enable --now opencode-memory.service
```

## Upstream Tracking

This project tracks changes from [claude-mem](https://github.com/thedotmack/claude-mem):

```bash
git remote add upstream https://github.com/thedotmack/claude-mem.git
git fetch upstream
git log upstream/main --oneline -20
```

## Development

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all
```

## License

MIT
