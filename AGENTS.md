# opencode-mem

Rust port of [claude-mem](https://github.com/thedotmack/claude-mem) for OpenCode.

## TARGET GOAL

**Full feature parity with claude-mem (TypeScript).**

Upstream: https://github.com/thedotmack/claude-mem
Last reviewed commit: `1341e93fcab15b9caf48bc947d8521b4a97515d8`

## Current Status: ~75% Complete

### Implemented

| Component | Status | Details |
|-----------|--------|---------|
| MCP Tools | ✅ 6/4 | search, timeline, get_observations, memory_get, memory_recent, memory_hybrid_search |
| Database | ✅ | SQLite + FTS5, migrations v1-v6 |
| CLI | ✅ 100% | serve, mcp, search, stats, projects, recent, get |
| HTTP API | ⚠️ 77% | 27/35 endpoints |
| Storage | ✅ 100% | Core tables, session_summaries, paginated queries, 14 unit tests |
| AI Agent | ✅ 100% | compress_to_observation(), generate_session_summary() in LLM crate |

### HTTP API Endpoints (27 total)

**Core (14):**
- `/health`, `/api/readiness`, `/api/version`
- `/observe`, `/search`, `/hybrid-search`
- `/observations/{id}`, `/observations/batch`, `/api/observations`
- `/api/summaries`, `/api/prompts`
- `/api/session/{id}`, `/api/prompt/{id}`
- `/recent`, `/timeline`, `/projects`, `/stats`

**Search (6):**
- `/api/search/observations`, `/api/search/by-type`, `/api/search/by-concept`
- `/api/search/sessions`, `/api/search/prompts`, `/api/search/by-file`

**Context (3):**
- `/api/context/recent`, `/context/inject`, `/session/summary`

**Events (1):**
- `/events` (SSE)

### NOT Implemented (Priority Order)

| # | Feature | Priority | Effort |
|---|---------|----------|--------|
| 1 | **Full HTTP API** — ~8 missing endpoints | HIGH | Medium |
| 2 | **Pending queue processing** — crash recovery logic | MEDIUM | Medium |
| 3 | **Hook system** — context, session-init, observation, summarize events | MEDIUM | Medium |
| 4 | **Web Viewer UI** — real-time stream on localhost:37777 | MEDIUM | Large |
| 5 | **Cursor/IDE hooks** — IDE integration | LOW | Medium |
| 6 | **Chroma vector DB** — semantic search (optional, FTS5 suffices) | LOW | Large |
| 7 | **Privacy tags** — `<private>` filtering | LOW | Small |

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

- `crates/storage/src/sqlite_monolith.rs` — main storage implementation (1161 lines)
- `crates/storage/src/tests.rs` — 14 unit tests (231 lines)
- `crates/storage/src/types.rs` — shared types (22 lines)
- `crates/storage/src/migrations.rs` — schema migrations v1-v6
- `crates/mcp/src/lib.rs` — MCP server with 6 tools
- `crates/http/src/lib.rs` — HTTP API endpoints (27 routes)
