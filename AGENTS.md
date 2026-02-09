# opencode-mem

Rust port of [claude-mem](https://github.com/thedotmack/claude-mem) for OpenCode.

## TARGET GOAL

**Full feature parity with claude-mem (TypeScript).**

Upstream: https://github.com/thedotmack/claude-mem
Last reviewed commit: `1341e93fcab15b9caf48bc947d8521b4a97515d8`

## Current Status: 100% Complete (Feature Parity Achieved)

### Implemented

| Component | Status | Details |
|-----------|--------|---------|
| MCP Tools | ✅ 17 tools | search, timeline, get_observations, memory_*, knowledge_*, infinite_*, save_memory |
| Database | ✅ | SQLite + FTS5, migrations v1-v10, r2d2 pool |
| CLI | ✅ 100% | serve, mcp, search, stats, projects, recent, get, hook (context, session-init, observe, summarize) |
| HTTP API | ✅ 100% | 65 endpoints (upstream has 56) |
| Storage | ✅ 100% | Core tables, session_summaries, pending queue |
| AI Agent | ✅ 100% | compress_to_observation(), generate_session_summary() |
| Web Viewer | ✅ 100% | Dark theme UI, SSE real-time updates |
| Privacy Tags | ✅ 100% | `<private>` content filtering |
| Pending Queue | ✅ 100% | Crash recovery, visibility timeout, dead letter queue |
| Hook System | ✅ 100% | CLI hooks: context, session-init, observe, summarize |
| Hybrid Capture | ✅ 100% | Per-turn observation via `session.idle` + debounce + batch endpoint |
| Project Exclusion | ✅ 100% | OPENCODE_MEM_EXCLUDED_PROJECTS env var, glob patterns, ~ expansion |
| Save Memory | ✅ 100% | Direct observation storage (MCP + HTTP), bypasses LLM compression |

### NOT Implemented

| # | Feature | Priority | Effort |
|---|---------|----------|--------|
| 1 | **Cursor/IDE hooks** — IDE integration | LOW | Medium |

### Experimental (Ready)

| Feature | Status | Notes |
|---------|--------|-------|
| **Infinite Memory** | ✅ Ready | PostgreSQL + pgvector backend for long-term AGI memory. Session isolation, hierarchical summaries (5min→hour→day), content truncation. Enabled via INFINITE_MEMORY_URL. **Raw events are NEVER deleted** — drill-down API allows zooming from any summary back to original events. |
| **Dynamic Memory** | ✅ Ready | Solves "static summaries" problem. **Deep Zoom:** 4 HTTP endpoints (`/api/infinite/expand_summary/:id`, `/time_range`, `/drill_hour/:id`, `/drill_minute/:id`) for drilling down from summaries to raw events. **Structured Metadata:** `SummaryEntities` (files, functions, libraries, errors, decisions) extracted via `response_format: json_object`. Enables fact-based search even when text summary is vague. |
| **Semantic Search** | ✅ Ready | fastembed-rs (AllMiniLML6V2, 384d) + sqlite-vec for cosine similarity. Hybrid search: FTS5 BM25 (50%) + vector similarity (50%). HTTP endpoint: `/semantic-search`. |

## Upstream Sync

- repo: thedotmack/claude-mem
- watch: src/services/, src/constants/
- ignore: *.test.ts, README*, docs/
- last_reviewed: b2ddf59db46cf380964379e9ba2d7279c67fc12b

## Architecture

```
crates/
├── core/        # Domain types (Observation, Session, etc.)
├── storage/     # SQLite + FTS5 + migrations
├── embeddings/  # Vector embeddings (fastembed AllMiniLML6V2)
├── search/      # Hybrid search (FTS + keyword + semantic)
├── llm/         # LLM compression (Antigravity API)
├── service/     # Business logic layer (ObservationService, SessionService)
├── http/        # HTTP API (Axum)
├── mcp/         # MCP server (stdio)
├── infinite-memory/ # PostgreSQL + pgvector backend
└── cli/         # CLI binary
```

## Key Files

- `crates/storage/src/storage/` — modular storage (mod.rs + 10 domain modules)
- `crates/storage/src/migrations/` — schema migrations v1-v10 (12 files)
- `crates/mcp/src/` — MCP server: lib.rs + tools.rs + handlers/
- `crates/http/src/handlers/` — HTTP handlers (11 modules)
- `crates/llm/src/` — AI agent: client, observation, summary, knowledge, insights
- `crates/infinite-memory/src/` — PostgreSQL + pgvector backend

## Tech Debt & Known Issues

None currently tracked.

### Resolved
- ~~Code Duplication in observation_service.rs~~ — extracted shared `persist_and_notify` method
- ~~Blocking I/O in observation_service.rs~~ — embedding calls wrapped in `spawn_blocking`
- ~~Data Loss on Update in knowledge.rs~~ — implemented provenance merging logic
- ~~SQLITE_LOCKED in knowledge.rs~~ — refactored to use transactions and `query_row`

