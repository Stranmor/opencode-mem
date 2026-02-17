# opencode-mem

Rust port of [claude-mem](https://github.com/thedotmack/claude-mem) for OpenCode.

## TARGET GOAL

**Full feature parity with claude-mem (TypeScript).**

Upstream: https://github.com/thedotmack/claude-mem
Last reviewed commit: `b2ddf59db46cf380964379e9ba2d7279c67fc12b`

## Current Status: Feature Parity Achieved (excluding IDE hooks)

### Implemented

| Component | Status | Details |
|-----------|--------|---------|
| MCP Tools | ✅ 17 tools | search, timeline, get_observations, memory_*, knowledge_*, infinite_*, save_memory |
| Database | ✅ | PostgreSQL primary (pgvector + tsvector/GIN) + SQLite fallback (FTS5 + sqlite-vec), enum dispatch |
| CLI | ✅ 100% | serve, mcp, search, stats, projects, recent, get, hook (context, session-init, observe, summarize) |
| HTTP API | ✅ 100% | 65 endpoints (upstream has 56) |
| Storage | ✅ 100% | Core tables, session_summaries, pending queue |
| AI Agent | ✅ 100% | compress_to_observation(), generate_session_summary() |
| Web Viewer | ✅ 100% | Dark theme UI, SSE real-time updates |
| Privacy Tags | ✅ 100% | `<private>` content filtering applied in all paths (save_memory, compress_and_save, store_infinite_memory) |
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
| **Semantic Search** | ✅ Ready | fastembed-rs (BGE-M3, 1024d, 100+ languages). SQLite: sqlite-vec. PostgreSQL: pgvector. Hybrid search: FTS BM25 (50%) + vector similarity (50%). HTTP endpoint: `/semantic-search?q=...`. |

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
├── embeddings/  # Vector embeddings (fastembed BGE-M3, 1024d, multilingual)
├── search/      # Hybrid search (FTS + keyword + semantic)
├── llm/         # LLM compression (Antigravity API)
├── service/     # Business logic layer (ObservationService, SessionService, QueueService)
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
- `crates/core/src/observation/low_value_filter.rs` — configurable noise filter (SPOT), env: `OPENCODE_MEM_FILTER_PATTERNS`
- `crates/llm/src/` — AI agent: client, observation, summary, knowledge, insights
- `crates/infinite-memory/src/` — PostgreSQL + pgvector backend

## Tech Debt & Known Issues

- Local HTTP server fails to start if port 37777 is already in use; stop the existing process before starting a new server.
- Gate MCP review tooling intermittently returns 429/502 or "Max retries exceeded", blocking automated code review.
- Test coverage: ~40% of critical paths. Service layer, HTTP handlers, infinite-memory, CLI still have zero tests.
- **Typed error enums partial** — Leaf crates have typed errors (`CoreError`, `EmbeddingError`, `LlmError`), but storage/service/http still use `anyhow::Result`. HTTP layer converts everything to 500 via blanket `From<anyhow::Error>`. Next step: `StorageError` in storage trait → `ServiceError` → `ApiError` `From` impls.
- ~~PG search_vec missing columns~~ — `search_vec` tsvector now includes `facts` (weight C) and `keywords` (weight D) via trigger function (generated column replaced due to subquery limitation)
- **Session observations not durable** — `POST /api/sessions/observations` uses `tokio::spawn` (ephemeral), while `/observe` uses persistent DB queue. Server crash loses session observations.
- **Knowledge save race condition** — No unique constraint on `global_knowledge.title`. Concurrent saves with same title create duplicates.
- ~~PG noise_level default mismatch~~ — PG migration default changed to `'medium'`, existing `'normal'` rows migrated
- **Migration skips embeddings** — `migrate` command (SQLite→PG) doesn't transfer vector embeddings. Semantic search breaks until manual backfill.
- ~~SQLite merge_into_existing deadlock risk~~ — `merge_into_existing` now uses `TransactionBehavior::Immediate` to prevent SQLITE_BUSY in WAL mode
- **Queue processor UUID collision** — UUID v5 based on `pending_messages.id` (DB row ID). If queue table is truncated while observations persist, new messages reuse old IDs → colliding UUIDs → silent data loss.
- ~~Privacy filter bypass in save_memory~~ — `filter_private_content` now applied in `save_memory` before observation creation
- ~~Blocking I/O in session_service~~ — replaced `std::process::Command` with `tokio::process::Command`
- **Phantom observation return after dedup merge** — `process` and `save_memory` return the local Observation even when merged into existing via dedup. Caller gets temporary ID that doesn't exist in DB.
- **Infinite memory compression pipeline starvation** — `run_full_compression` fetches fixed batch (100) of unaggregated summaries across sessions. If distributed across many sessions, no single session meets threshold → pipeline stalls.
- ~~PG search_by_file uses LIKE on JSONB text~~ — `search_by_file` now uses `jsonb_array_elements_text` to search within JSONB arrays properly
- ~~merge_into_existing incomplete field update~~ — Updates all merge-relevant fields including `noise_reason`, `prompt_number`, `discovery_tokens`
- **PG save_observation dead error handling** — `ON CONFLICT (id) DO NOTHING` suppresses constraint violation, making the explicit SQLSTATE 23505 error match arm unreachable.
- **Privacy filter fallback leaks unfiltered data** — ~~In `store_infinite_memory` and `compress_and_save`, `serde_json::from_str(&filtered).unwrap_or(tool_call.input.clone())` falls back to the original unfiltered input if the filter corrupts JSON structure, bypassing the security filter entirely.~~ **RESOLVED:** Fallback now returns `Value::Null` with warning log instead of unfiltered input.
- **Sequential LLM calls in observation process** — `extract_knowledge` and `store_infinite_memory` are awaited sequentially in `process()`, adding latency to the critical path. Should be spawned as background tasks.
- **Injection cleanup only on startup** — `cleanup_old_injections` runs only in `run_startup_recovery`. Long-running servers accumulate stale injection records until next restart. Should run periodically (e.g., hourly in background processor loop).
- **Unbounded injected IDs per session** — No cap on how many observation IDs are recorded per session via `/context/inject`. A project with 10K observations causes 10K embeddings loaded into memory per echo check (~40MB). Should cap at ~500 or paginate comparison.
- **PG get_embeddings_for_ids no batching** — PG implementation passes all IDs via `ANY($1)` without chunking, unlike SQLite which uses `MAX_BATCH_IDS`. Could hit PG parameter limits for very large ID sets.
- **CUDA GPU acceleration blocked on Pascal** — ort-sys 2.0.0-rc.11 pre-built CUDA provider includes only SM 7.0+ (Volta+). GTX 1060 (SM 6.1 Pascal) gets `cudaErrorSymbolNotFound` at inference. CUDA EP registers successfully but all inference ops fail. CUDA 12 compat libs installed at `/opt/cuda-12-compat/lib/` on home-server. Workaround: CPU-only embeddings with `OPENCODE_MEM_DISABLE_EMBEDDINGS=1` for throttling. To resolve: either build ONNX Runtime from source with `CMAKE_CUDA_ARCHITECTURES=61`, or upgrade to Volta+ GPU.
- ~~No DB path env var~~ — `get_db_path()` checks `OPENCODE_MEM_DB_PATH` env var before falling back to default path

### Resolved
- ~~Code Duplication in observation_service.rs~~ — extracted shared `persist_and_notify` method
- ~~Blocking I/O in observation_service.rs~~ — embedding calls wrapped in `spawn_blocking`
- ~~Data Loss on Update in knowledge.rs~~ — implemented provenance merging logic
- ~~SQLITE_LOCKED in knowledge.rs~~ — refactored to use transactions and `query_row`
- ~~Hardcoded filter patterns~~ — extracted to `low_value_filter.rs` with `OPENCODE_MEM_FILTER_PATTERNS` env support
- ~~Pre-commit hooks fail on LLM integration tests~~ — marked with `#[ignore]`, run explicitly via `cargo test -- --ignored`
- ~~Silent data loss in embedding storage~~ — atomic DELETE+INSERT via transaction
- ~~PG/SQLite dedup divergence~~ — SQLSTATE 23505 caught, returns Ok(false) matching SQLite behavior
- ~~SQLite crash durability~~ — PRAGMA synchronous = FULL
- ~~Silent data fabrication in type parsing~~ — 18 enum parsers now log warnings on invalid values
- ~~Silent error swallowing in service layer~~ — knowledge extraction, infinite memory errors at warn level
- ~~LLM empty summary stored silently~~ — now returns error, prevents hierarchy corruption
- ~~Env var parse failures invisible~~ — `env_parse_with_default` helper logs warnings
- ~~MCP handlers accept empty required fields~~ — now reject with error
- ~~MCP stdout silent write failures~~ — error paths now break cleanly
- ~~Unbounded query limits (DoS)~~ — SearchQuery/Timeline/Pagination capped at 1000, BatchRequest.ids at 500
- ~~pg_storage.rs monolith (1826 lines)~~ — split into modular directory: mod.rs + 9 domain modules
- ~~4-way SPOT violation in save+embed~~ — centralized through ObservationService::save_observation()
- ~~Blocking async in embedding calls~~ — wrapped in spawn_blocking
- ~~\~70 unsafe `as` casts in pg_storage~~ — replaced with TryFrom/checked conversions (3 intentional casts with #[allow(reason)])
- ~~sqlite_async.rs boilerplate (62 self.clone())~~ — delegate! macro, 483→336 lines
- ~~Zero-vector embedding corruption~~ — guard in store_embedding + find_similar (both SQLite + PG)
- ~~Stale embedding after dedup merge~~ — re-generate from merged content
- ~~merge_into_existing not transactional (SQLite)~~ — wrapped in transaction
- ~~Nullable project crash in session summary (PG)~~ — handle Option<Option<String>> correctly
- ~~Infinite Memory missing schema initialization~~ — added `migrations.rs` with auto-run in `InfiniteMemory::new()`
- ~~Privacy leak: tool inputs stored unfiltered in infinite memory~~ — `filter_private_content` applied to inputs before storage
- ~~SPOT: ObservationType/NoiseLevel parsing duplicated 10+ times~~ — extracted `parse_pg_observation_type()` and `parse_pg_noise_level()` helpers
- ~~SPOT: hybrid_search_v2 inline row parsing copy-pasted from row_to_search_result~~ — uses `row_to_search_result_with_score` now
- ~~SPOT: map_search_result + map_search_result_default_score duplicate~~ — merged into single function with `score_col: Option<usize>`
- ~~rebuild_embeddings false claim about automatic re-embedding~~ — now states to run backfill CLI command
- ~~Inconsistent default query limits (20/50/10) across HTTP/MCP~~ — unified via `DEFAULT_QUERY_LIMIT` in core/constants.rs
- ~~MCP tool name unwrap_or("") silently accepts empty~~ — explicit rejection with error message
- ~~Score fabrication: missing score defaulted to 1.0 (perfect)~~ — changed to 0.0 (no match)
- ~~MAX_BATCH_IDS duplicated between http and mcp~~ — unified in core/constants.rs
- ~~5x Regex::new().unwrap() in import_insights.rs~~ — wrapped in LazyLock statics
- ~~LlmClient::new() panics on TLS init failure~~ — returns Result, callers handle with ?
- ~~Unsafe `as` casts in pipeline.rs, pending.rs~~ — replaced with TryFrom/checked conversions
- ~~HTTP→Service layer violation~~ — all 9 handlers migrated to use `QueueService`, `SessionService`, `SearchService`. Zero direct `state.storage.*` calls in handlers.
- ~~dedup_tests.rs monolith (1050 lines)~~ — split into 4 modules: union_dedup_tests, merge_tests, find_similar_tests, embedding_text_tests. 3 misplaced tests relocated.
- ~~StoredEvent.event_type String~~ — changed to `EventType` enum with `FromStr` parser. Unknown types logged as warning and skipped (Zero Fallback).
- ~~No newtypes for prompt_number/discovery_tokens~~ — `PromptNumber(u32)` and `DiscoveryTokens(u32)` newtypes in core, used in Observation, SessionSummary, UserPrompt.
- ~~No typed error enums in leaf crates~~ — `CoreError` (4 variants), `EmbeddingError` (4 variants), `LlmError` (7 variants with `is_transient()`) defined and used in public APIs.
- ~~SQLite timeline query missing noise_level~~ — all 4 timeline SELECT variants selected only 4 columns but `map_search_result` read column index 4 for `noise_level`. Every row silently failed → empty timeline. Added `noise_level` to all timeline queries.
- ~~PG search/hybrid_search empty-query fallback type mismatch~~ — fallback returned `Vec<Observation>` where `Vec<SearchResult>` was expected. Wrapped with `SearchResult::from_observation`.
- ~~PG knowledge usage_count INT4 vs i64 mismatch~~ — `global_knowledge.usage_count` was `INT4` in PG but decoded as `i64`. ALTERed column to `BIGINT`.
- ~~Memory injection recursion~~ — observe hook re-processed `<memory-*>` blocks injected by IDE plugin, creating duplicate observations that got re-injected in a loop. Added `filter_injected_memory()` at all entry points: HTTP observe/observe_batch (before queue), session observation endpoints, CLI hook observe, save_memory, plus service-layer defense-in-depth.
- ~~Dedup threshold env var without bounds validation~~ — `OPENCODE_MEM_DEDUP_THRESHOLD` and `OPENCODE_MEM_INJECTION_DEDUP_THRESHOLD` now clamped to [0.0, 1.0] on parse. Values outside cosine similarity range no longer silently disable detection.
