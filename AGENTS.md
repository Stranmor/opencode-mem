# opencode-mem

Rust port of [claude-mem](https://github.com/thedotmack/claude-mem) for OpenCode.

## TARGET GOAL

**Full feature parity with claude-mem (TypeScript).**

Upstream: https://github.com/thedotmack/claude-mem
Last reviewed commit: `1341e93fcab15b9caf48bc947d8521b4a97515d8`

## Current Status: ~98% Complete

### Implemented

| Component | Status | Details |
|-----------|--------|---------|
| MCP Tools | ✅ 6 tools | search, timeline, get_observations, memory_get, memory_recent, memory_hybrid_search |
| Database | ✅ | SQLite + FTS5, migrations v1-v7 |
| CLI | ✅ 100% | serve, mcp, search, stats, projects, recent, get, hook (context, session-init, observe, summarize) |
| HTTP API | ✅ 100% | 64 endpoints (upstream has 56) |
| Storage | ✅ 100% | Core tables, session_summaries, pending queue, 1481 lines |
| AI Agent | ✅ 100% | compress_to_observation(), generate_session_summary() |
| Web Viewer | ✅ 100% | Dark theme UI, SSE real-time updates |
| Privacy Tags | ✅ 100% | `<private>` content filtering |
| Pending Queue | ✅ 100% | Crash recovery, visibility timeout, dead letter queue |
| Hook System | ✅ 100% | CLI hooks: context, session-init, observe, summarize |

### NOT Implemented

| # | Feature | Priority | Effort |
|---|---------|----------|--------|
| 1 | **Cursor/IDE hooks** — IDE integration | LOW | Medium |

### Experimental (WIP)

| Feature | Status | Notes |
|---------|--------|-------|
| **Infinite Memory** | 🚧 WIP | PostgreSQL + pgvector backend for long-term AGI memory. Has known issues: session mixing in compression pipeline, no hour/day summaries. Disabled by default (INFINITE_MEMORY_URL not set). |

## Upstream Sync

- repo: thedotmack/claude-mem
- watch: src/services/, src/constants/
- ignore: *.test.ts, README*, docs/
- last_reviewed: 1341e93fcab15b9caf48bc947d8521b4a97515d8

## Architecture

```
crates/
├── core/        # Domain types (Observation, Session, etc.)
├── storage/     # SQLite + FTS5 + migrations
├── embeddings/  # Vector embeddings (stub, optional)
├── search/      # Hybrid search (FTS + keyword)
├── llm/         # LLM compression (Antigravity API)
├── http/        # HTTP API (Axum)
├── mcp/         # MCP server (stdio)
├── plugin/      # OpenCode plugin hooks
└── cli/         # CLI binary
```

## Key Files

- `crates/storage/src/sqlite_monolith.rs` — main storage implementation (1481 lines)
- `crates/storage/src/tests.rs` — 14 unit tests (231 lines)
- `crates/storage/src/types.rs` — shared types (22 lines)
- `crates/storage/src/migrations.rs` — schema migrations v1-v7
- `crates/mcp/src/lib.rs` — MCP server with 6 tools
- `crates/http/src/lib.rs` — HTTP API endpoints (64 routes)
- `crates/http/src/viewer.rs` — Web Viewer UI module
- `crates/http/src/viewer.html` — Dark theme viewer template
