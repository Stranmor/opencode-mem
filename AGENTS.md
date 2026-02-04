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
| Database | ✅ | SQLite + FTS5, migrations v1-v10, r2d2 pool |
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
| **Infinite Memory** | ✅ Ready | PostgreSQL + pgvector backend for long-term AGI memory. Session isolation, hierarchical summaries (5min→hour→day), content truncation. Enabled via INFINITE_MEMORY_URL. **Raw events are NEVER deleted** — drill-down API allows zooming from any summary back to original events. |
| **Dynamic Memory** | ✅ Ready | Solves "static summaries" problem. **Deep Zoom:** 4 HTTP endpoints (`/api/infinite/expand_summary/:id`, `/time_range`, `/drill_hour/:id`, `/drill_day/:id`) for drilling down from summaries to raw events. **Structured Metadata:** `SummaryEntities` (files, functions, libraries, errors, decisions) extracted via `response_format: json_object`. Enables fact-based search even when text summary is vague. |
| **Semantic Search** | 🚧 WIP | Vector embeddings (fastembed-rs) + sqlite-vec for cosine similarity. Hybrid search combines FTS5 (text) + semantic (meaning). Solves: "проблема с потоками" → finds "race condition". |

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

- `crates/storage/src/storage/` — modular storage (mod.rs + 10 domain modules)
- `crates/storage/src/migrations/` — schema migrations v1-v10 (12 files)
- `crates/mcp/src/` — MCP server: lib.rs + tools.rs + handlers.rs
- `crates/http/src/handlers/` — HTTP handlers (10 modules)
- `crates/llm/src/` — AI agent: client, observation, summary, knowledge
- `crates/infinite-memory/src/` — PostgreSQL + pgvector backend
