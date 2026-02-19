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
| Database | ✅ | PostgreSQL only (pgvector + tsvector/GIN), direct PgStorage (no dispatch enum) |
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
| **Semantic Search** | ✅ Ready | fastembed-rs (BGE-M3, 1024d, 100+ languages). PostgreSQL: pgvector. Hybrid search: FTS BM25 (50%) + vector similarity (50%). HTTP endpoint: `/semantic-search?q=...`. |

## Upstream Sync

- repo: thedotmack/claude-mem
- watch: src/services/, src/constants/
- ignore: *.test.ts, README*, docs/
- last_reviewed: eea4f599c0c54eb8d7dcc0d81a9364f2302fd1e6

## Architecture

```
crates/
├── core/        # Domain types (Observation, Session, etc.)
├── storage/     # PostgreSQL + pgvector + migrations
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

- `crates/storage/src/pg_storage/` — modular PG storage (mod.rs + domain modules)
- `crates/storage/src/pg_migrations.rs` — PostgreSQL schema migrations
- `crates/mcp/src/` — MCP server: lib.rs + tools.rs + handlers/
- `crates/http/src/handlers/` — HTTP handlers (11 modules)
- `crates/core/src/observation/low_value_filter.rs` — configurable noise filter (SPOT), env: `OPENCODE_MEM_FILTER_PATTERNS`
- `crates/llm/src/` — AI agent: client, observation, summary, knowledge, insights

## ADR: Context-Aware Compression (CREATE/UPDATE/SKIP)

### Problem
LLM always creates NEW observations even when near-identical ones exist. The `existing_titles` dedup hint in the prompt only lists titles — the LLM lacks enough context to confidently mark duplicates as negligible. Post-facto dedup (cosine similarity merge, background sweep) catches some duplicates but is fundamentally reactive: duplicates are created, then cleaned.

### Constraints
- API calls are free (zero cost concern)
- ~100 observations in DB, growing slowly
- FTS + GIN index exists on `search_vec` tsvector column
- `response_format: json_object` is required for all LLM calls
- Raw events preserved in Infinite Memory (observations are derived views)

### Decision: Enrich compression prompt with candidate observations, let LLM decide CREATE/UPDATE/SKIP

**One code path, three outcomes.** Before LLM compression, retrieve top 5 candidate observations via FTS on raw input text. Feed their full content to the LLM alongside the new tool interaction. LLM returns a discriminated result:

- **CREATE**: Genuinely new knowledge → full observation (current behavior)
- **UPDATE(target_id)**: Refines existing observation → full replacement of target's content fields
- **SKIP**: Zero new information → log and discard

### Alternatives Considered

1. **Merge-or-create gate (separate pre-search → conditional prompt branching)** — Rejected. Two code paths, two prompt templates, patch parsing, false positive recovery. Over-engineered for a context starvation problem.

2. **Aggressive post-facto dedup only (lower thresholds, more frequent sweeps)** — Rejected. Treats symptoms. Duplicates still created, wasting LLM calls and storage churn.

3. **Pre-embed raw input for semantic search** — Rejected. Raw tool output has low similarity to compressed observations (different vocabulary, length, structure). FTS keyword matching is sufficient for candidate retrieval.

### Implementation Plan

**Phase 1: Candidate retrieval** (in `ObservationService`, before LLM call)
- Extract keywords from raw input via `plainto_tsquery`
- Query `observations` via existing `search_vec` GIN index, limit 5
- Also include 2-3 most recent observations from same session (most likely merge targets)

**Phase 2: Prompt modification** (`crates/llm/src/observation.rs`)
- Replace `existing_titles` with full candidate observations (id + title + narrative + facts)
- Add DECISION section: CREATE / UPDATE(id) / SKIP
- Response schema becomes discriminated union with `action` field

**Phase 3: Response handling** (`crates/service/src/observation_service/`)
- Parse `action` field from LLM response
- CREATE → existing `persist_and_notify` path
- UPDATE → validate target_id exists and was in candidate set → full field replacement → regenerate embedding
- SKIP → log at debug, return early

**Phase 4: Simplify dedup layers**
- Post-facto cosine dedup becomes safety net only (keep threshold at 0.85)
- Background sweep remains as defense-in-depth (30 min interval)
- Echo detection (0.80) stays unchanged (different purpose: injection recursion prevention)

### Watch Out For
- **Candidate retrieval quality**: if FTS misses the relevant observation, LLM creates a duplicate. Mitigate by also including recent same-session observations.
- **LLM hallucinating target_id**: validate returned ID exists AND was in candidate set. If not → treat as CREATE.
- **Token growth**: 5 candidates × ~500 tokens = ~2500 extra tokens per call. Negligible for current models.
- `crates/infinite-memory/src/` — PostgreSQL + pgvector backend

## Tech Debt & Known Issues

- Local HTTP server fails to start if port 37777 is already in use; stop the existing process before starting a new server.
- Gate MCP review tooling intermittently returns 429/502 or "Max retries exceeded", blocking automated code review.
- Test coverage: ~40% of critical paths. Service layer, HTTP handlers, infinite-memory, CLI still have zero tests.
- ~~Typed error enums partial~~ — All crates now use typed errors: `CoreError`, `EmbeddingError`, `LlmError`, `StorageError` (5 variants), `ServiceError` (8 variants). `From<ServiceError> for ApiError` maps errors to proper HTTP status codes (404, 422, 400, 503). CLI uses `anyhow` at binary boundary (idiomatic).
- **Permissive CORS on HTTP server** — `CorsLayer::permissive()` in `crates/http/src/lib.rs` allows any website to read private memory data from `localhost:37777`. Should use strict origin allowlist or remove CORS entirely (local IDE plugins don't need it).
- **Infinite Memory drops filtered observations** — `ObservationService::process` only calls `store_infinite_memory` if `compress_and_save` returns a valid observation. Filtered (low-value) or deduplicated observations are NOT stored in Infinite Memory, violating the "raw events are NEVER deleted" goal.
- **pg_migrations non-transactional** — `run_pg_migrations()` runs 30+ SQL statements without a wrapping transaction. Partial failure leaves DB in inconsistent state (mitigated by idempotent design — re-run converges).
- **pg_migrations .ok() swallows index errors** — `idx_obs_embedding` creation uses `.ok()` which silently swallows all errors, not just "too few rows for ivfflat". Connection errors or missing extension go unnoticed.
- **KnowledgeType SPOT violation** — `KnowledgeType` enum variants are hardcoded separately in Rust enum, LLM prompts, and MCP tool schemas. Adding a new type requires manual updates in 3 places.
- **strip_markdown_json duplicated** — Implemented in both `crates/core/src/json_utils.rs` and locally in `crates/llm/src/insights.rs`. The LLM crate already depends on core.
- **save_memory bypasses Infinite Memory** — `ObservationService::save_memory` calls `persist_and_notify` directly, bypassing `store_infinite_memory`. Manually added memories don't appear in infinite timeline.
- **admin_restart uses exit(0)** — `admin_restart` handler calls `exit(0)`, which only triggers restart with `Restart=always` systemd config (not the default `Restart=on-failure`). Should use exit code 1.
- **Infinite Memory migrations not evolutionary** — `crates/infinite-memory/src/migrations.rs` uses `CREATE TABLE IF NOT EXISTS` but lacks `ALTER TABLE ... ADD COLUMN IF NOT EXISTS` for columns added later (e.g., `entities`). Database created with older version will crash at runtime when querying missing columns.
- **Infinite Memory double connection pool** — `InfiniteMemory::new()` creates its own `PgPool` from `DATABASE_URL`, duplicating the pool created by `StorageBackend`. Doubles connection overhead and splits connection limit. Should accept an existing `PgPool` reference instead.
- **search_by_file missing GIN index** — `files_read` and `files_modified` JSONB columns have no GIN index, causing full table scans for file search queries.
- **parse_pg_vector_text string parsing overhead** — Vector data from PG is parsed via string splitting instead of binary `pgvector` crate deserialization. CPU overhead on search/retrieval.
- **Hybrid search stop-words fallback** — If `hybrid_search` receives query with only stop words (empty tsquery), falls back to `get_recent` observations instead of returning empty results.
- **sqlx missing uuid feature** — `crates/storage/Cargo.toml` sqlx dep lacks `uuid` feature flag. Works because uuid values are extracted manually from PgRow, not via `query_as!`. Adding the feature would enable compile-time checked queries.
- **sqlx missing TLS feature** — `crates/storage/Cargo.toml` sqlx dep lacks TLS feature (`tls-rustls`). Works because connection goes through localhost SSH tunnel (no SSL). Would fail if connecting directly to SSL-enabled PG.
- ~~PG search_vec missing columns~~ — `search_vec` tsvector now includes `facts` (weight C) and `keywords` (weight D) via trigger function (generated column replaced due to subquery limitation)
- ~~Session observations not durable~~ — `POST /api/sessions/observations` now uses persistent DB queue (`queue_service.queue_message()`) instead of ephemeral `tokio::spawn`. Server crash no longer loses session observations.
- ~~Knowledge save race condition~~ — Unique index `idx_knowledge_title_unique` on `LOWER(title)` (PG). Save methods retry on constraint violation, merging via SELECT+UPDATE on retry.
- ~~PG noise_level default mismatch~~ — PG migration default changed to `'medium'`, existing `'normal'` rows migrated
- ~~Migration command removed~~ — SQLite→PG `migrate` command deleted (PG is now the only backend)

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
- ~~PG get_embeddings_for_ids no batching~~ — PG implementation now chunks IDs via `MAX_BATCH_IDS`
- **CUDA GPU acceleration blocked on Pascal** — ort-sys 2.0.0-rc.11 pre-built CUDA provider includes only SM 7.0+ (Volta+). GTX 1060 (SM 6.1 Pascal) gets `cudaErrorSymbolNotFound` at inference. CUDA EP registers successfully but all inference ops fail. CUDA 12 compat libs cleaned up from home-server. Workaround: CPU-only embeddings with `OPENCODE_MEM_DISABLE_EMBEDDINGS=1` for throttling. To resolve: either build ONNX Runtime from source with `CMAKE_CUDA_ARCHITECTURES=61`, or upgrade to Volta+ GPU.
- **`std::env::set_var` will become unsafe in edition 2024** — `EmbeddingService::new()` calls `set_var("OMP_NUM_THREADS")` inside `Once::call_once`. Safe in edition 2021, but requires `unsafe {}` block when migrating to edition 2024. Alternative: move env var setting to CLI `main()` before tokio runtime init.
- ~~sqlite-vec vec0 1024 record limit~~ — SQLite backend removed entirely. PG-only now (pgvector has no such limit).
- ~~No DB path env var~~ — SQLite backend removed. `DATABASE_URL` is the only config needed.
- ~~Session summarization uses wrong ID for API sessions~~ — `summarize_session` now queries via `session_id` (UUID) instead of `content_session_id` (IDE ID)
- ~~PG save_observation title dedup mismatch~~ — PG `save_observation` catches SQLSTATE 23505 (`idx_obs_title_norm` unique constraint) and returns `Ok(false)`, idempotent duplicate handling
- ~~AppState semaphore ignored~~ — `process_pending_queue` and `start_background_processor` now use shared `AppState.semaphore` instead of creating local semaphores
- ~~SPOT: dual-database global_knowledge~~ — SQLite backend removed entirely. Knowledge ops now PG-only.
- ~~memory_knowledge_delete MCP tool broken~~ — SQLite backend removed. Knowledge operations use PG directly.
- ~~SQLite vs PG enum parse divergence~~ — SQLite backend removed. PG is the only backend.
- **Inconsistent HTTP error responses** — Some handlers (`infinite.rs`, `queue.rs`) return structured JSON error bodies `{"error": "..."}`, others return bare HTTP status codes with no body. Unified `ApiError` pattern needed.
- **Pagination limit inconsistency** — Pagination endpoints (`/api/observations`, `/api/summaries`) hardcode cap of 100, while search/recent endpoints use `MAX_QUERY_LIMIT` (1000). Should use a single constant or define explicit `MAX_PAGINATION_LIMIT`.
- **save_memory ambiguous 422** — Returns `422 Unprocessable Entity` for both 'low-value filtered' AND 'exact duplicate' observations. Clients cannot distinguish. Should return `200 OK` for duplicates (idempotent) and `422` for filtered only.
- **CLI hook.rs swallows JSON parse errors** — `get_project_from_stdin` and `build_observation_request` silently resolve to `None` on malformed JSON, masking the actual syntax error.
- **import_insights.rs regex truncation** — `OBSERVATION_RE`, `IMPLICATION_RE`, `RECOMMENDATION_RE` use `.+` (no newlines). Multi-line fields lose content after first line.
- **import_insights.rs title_exists swallows DB errors** — `unwrap_or(false)` on `search_knowledge` result. DB failure → proceeds to insert instead of propagating error.
- **session_service hardcoded `opencode` binary** — `summarize_session_from_export` calls `Command::new("opencode")` assuming PATH availability. Should be configurable.
- **session_service inconsistent empty-session handling** — `summarize_session` returns early on empty observations leaving status unchanged; `complete_session` correctly marks as `Completed`.
- **build_tsquery empty string crash** — `build_tsquery` in `pg_storage/mod.rs` returns empty string for symbol-only input. PostgreSQL's `to_tsquery('')` throws a syntax error.
- **parse_json_value unnecessary clone** — Takes `&serde_json::Value` and clones internally. Callers own the value; should take by value.
- **get_all_pending_messages unfiltered** — Returns all rows including 'failed' and 'processing' statuses, contradicting function name and `get_pending_count` logic.
- **get_queue_stats dead 'processed' count** — Queries for `status = 'processed'` but `complete_message` deletes rows. Count is always zero.

### Resolved
- ~~Infinite memory compression pipeline starvation~~ — `run_full_compression` now queries per-session via `get_sessions_with_unaggregated_*` + `get_unaggregated_*_for_session`, eliminating fixed cross-session batch that caused threshold starvation
- ~~Code Duplication in observation_service.rs~~ — extracted shared `persist_and_notify` method
- ~~Blocking I/O in observation_service.rs~~ — embedding calls wrapped in `spawn_blocking`
- ~~Data Loss on Update in knowledge.rs~~ — implemented provenance merging logic
- ~~SQLITE_LOCKED in knowledge.rs~~ — SQLite backend removed
- ~~Hardcoded filter patterns~~ — extracted to `low_value_filter.rs` with `OPENCODE_MEM_FILTER_PATTERNS` env support
- ~~Pre-commit hooks fail on LLM integration tests~~ — marked with `#[ignore]`, run explicitly via `cargo test -- --ignored`
- ~~Silent data loss in embedding storage~~ — atomic DELETE+INSERT via transaction
- ~~PG/SQLite dedup divergence~~ — SQLite backend removed
- ~~SQLite crash durability~~ — SQLite backend removed
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
- ~~sqlite_async.rs boilerplate (62 self.clone())~~ — SQLite backend removed entirely
- ~~Zero-vector embedding corruption~~ — guard in store_embedding + find_similar
- ~~Stale embedding after dedup merge~~ — re-generate from merged content
- ~~merge_into_existing not transactional (SQLite)~~ — SQLite backend removed
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
- ~~SQLite timeline query missing noise_level~~ — SQLite backend removed
- ~~PG search/hybrid_search empty-query fallback type mismatch~~ — fallback returned `Vec<Observation>` where `Vec<SearchResult>` was expected. Wrapped with `SearchResult::from_observation`.
- ~~PG knowledge usage_count INT4 vs i64 mismatch~~ — `global_knowledge.usage_count` was `INT4` in PG but decoded as `i64`. ALTERed column to `BIGINT`.
- ~~Memory injection recursion~~ — observe hook re-processed `<memory-*>` blocks injected by IDE plugin, creating duplicate observations that got re-injected in a loop. Added `filter_injected_memory()` at all entry points: HTTP observe/observe_batch (before queue), session observation endpoints, CLI hook observe, save_memory, plus service-layer defense-in-depth.
- ~~Dedup threshold env var without bounds validation~~ — `OPENCODE_MEM_DEDUP_THRESHOLD` and `OPENCODE_MEM_INJECTION_DEDUP_THRESHOLD` now clamped to [0.0, 1.0] on parse. Values outside cosine similarity range no longer silently disable detection.
- ~~NaN/Inf embedding validation~~ — `store_embedding` accepts NaN/Infinity vectors. Added `contains_non_finite()` guard. Non-finite floats now rejected with error.
- ~~SQLite PRAGMA synchronous mismatch~~ — SQLite backend removed
