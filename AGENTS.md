# opencode-mem

Rust port of [claude-mem](https://github.com/thedotmack/claude-mem) for OpenCode.

## TARGET GOAL

**Full feature parity with claude-mem (TypeScript).**

Upstream: https://github.com/thedotmack/claude-mem
Last reviewed commit: `eea4f599c0c54eb8d7dcc0d81a9364f2302fd1e6`

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
- last_reviewed: eea4f599c0c54eb8d7dcc0d81a9364f2302fd1e6

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
- ~~Session observations not durable~~ — `POST /api/sessions/observations` now uses persistent DB queue (`queue_service.queue_message()`) instead of ephemeral `tokio::spawn`. Server crash no longer loses session observations.
- ~~Knowledge save race condition~~ — Unique index `idx_knowledge_title_unique` on `LOWER(title)` (PG) / `title COLLATE NOCASE` (SQLite). Save methods retry on constraint violation, merging via SELECT+UPDATE on retry.
- ~~PG noise_level default mismatch~~ — PG migration default changed to `'medium'`, existing `'normal'` rows migrated
- **Migration skips embeddings** — `migrate` command (SQLite→PG) doesn't transfer vector embeddings. Semantic search breaks until manual backfill.
- ~~SQLite merge_into_existing deadlock risk~~ — `merge_into_existing` now uses `TransactionBehavior::Immediate` to prevent SQLITE_BUSY in WAL mode
- ~~Queue processor UUID collision~~ — UUID v5 now generated from content hash (tool_name + session_id + tool_response + created_at_epoch) instead of DB row ID. Table truncation no longer causes UUID collisions.
- ~~Privacy filter bypass in save_memory~~ — `filter_private_content` now applied in `save_memory` before observation creation
- ~~Blocking I/O in session_service~~ — replaced `std::process::Command` with `tokio::process::Command`
- ~~Phantom observation return after dedup merge~~ — `persist_and_notify` now returns `Option<(Observation, bool)>`: after dedup merge, fetches the actual merged observation from DB via `get_by_id`. Callers (`process`, `save_memory`, `compress_and_save`) receive the real persisted entity, not a phantom with temporary ID.
- ~~PG search_by_file uses LIKE on JSONB text~~ — `search_by_file` now uses `jsonb_array_elements_text` to search within JSONB arrays properly
- ~~merge_into_existing incomplete field update~~ — Updates all merge-relevant fields including `noise_reason`, `prompt_number`, `discovery_tokens`
- ~~PG save_observation dead error handling~~ — Removed unreachable SQLSTATE 23505 match arm; `ON CONFLICT (id) DO NOTHING` already handles duplicates via `rows_affected() == 0` → `Ok(false)`.
- ~~Privacy filter fallback leaks unfiltered data~~ — Fallback now returns `Value::Null` with warning log instead of unfiltered input.
- ~~Sequential LLM calls in observation process~~ — `extract_knowledge` and `store_infinite_memory` now run concurrently via `tokio::join!`
- ~~Injection cleanup only on startup~~ — `cleanup_old_injections` now runs periodically (~hourly) in the background processor loop
- ~~Unbounded injected IDs per session~~ — Capped at 500 most recent IDs per session via `MAX_INJECTED_IDS` constant
- ~~PG get_embeddings_for_ids no batching~~ — PG implementation now chunks IDs via `MAX_BATCH_IDS`, matching SQLite behavior
- **CUDA GPU acceleration blocked on Pascal** — ort-sys 2.0.0-rc.11 pre-built CUDA provider includes only SM 7.0+ (Volta+). GTX 1060 (SM 6.1 Pascal) gets `cudaErrorSymbolNotFound` at inference. CUDA EP registers successfully but all inference ops fail. CUDA 12 compat libs cleaned up from home-server. Workaround: CPU-only embeddings with `OPENCODE_MEM_DISABLE_EMBEDDINGS=1` for throttling. To resolve: either build ONNX Runtime from source with `CMAKE_CUDA_ARCHITECTURES=61`, or upgrade to Volta+ GPU.
- **sqlite-vec vec0 1024 record limit** — sqlite-vec (0.1.7-alpha.10) vec0 virtual table fails with `Could not insert a new vector chunk` after exactly 1024 records. Default `chunk_size=1024` cannot be overridden (the Rust crate's bundled C code ignores `chunk_size` parameter). Current coverage: 1024/1165 observations (87.9%). Workaround: migrate to PostgreSQL backend (pgvector has no such limit). When >2000 observations accumulate, this becomes blocking.
- ~~No DB path env var~~ — `OPENCODE_MEM_DB_PATH` env var now overrides default `~/.local/share/opencode-memory/memory.db` in `get_db_path()`.

### Resolved
- ~~Infinite memory compression pipeline starvation~~ — `run_full_compression` now queries per-session via `get_sessions_with_unaggregated_*` + `get_unaggregated_*_for_session`, eliminating fixed cross-session batch that caused threshold starvation
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
