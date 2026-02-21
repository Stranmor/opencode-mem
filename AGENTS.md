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
- **CUDA GPU acceleration blocked on Pascal** — ort-sys 2.0.0-rc.11 pre-built CUDA provider includes only SM 7.0+ (Volta+). GTX 1060 (SM 6.1 Pascal) gets `cudaErrorSymbolNotFound` at inference. CUDA EP registers successfully but all inference ops fail. CUDA 12 compat libs cleaned up from home-server. Workaround: CPU-only embeddings with `OPENCODE_MEM_DISABLE_EMBEDDINGS=1` for throttling. To resolve: either build ONNX Runtime from source with `CMAKE_CUDA_ARCHITECTURES=61`, or upgrade to Volta+ GPU.
- **`std::env::set_var` will become unsafe in edition 2024** — `EmbeddingService::new()` calls `set_var("OMP_NUM_THREADS")` inside `Once::call_once`. Safe in edition 2021, but requires `unsafe {}` block when migrating to edition 2024. Alternative: move env var setting to CLI `main()` before tokio runtime init.

### Resolved
- ~~Permissive CORS on HTTP server~~ — fixed by removing CorsLayer
- ~~Infinite Memory drops filtered observations~~ — fixed by calling `store_infinite_memory` concurrently with `compress_and_save`
- ~~pg_migrations non-transactional~~ — fixed
- ~~pg_migrations .ok() swallows index errors~~ — fixed
- ~~KnowledgeType SPOT violation~~ — fixed
- ~~strip_markdown_json duplicated~~ — fixed
- ~~save_memory bypasses Infinite Memory~~ — fixed
- ~~admin_restart uses exit(0)~~ — fixed
- ~~Infinite Memory migrations not evolutionary~~ — fixed
- ~~Infinite Memory double connection pool~~ — fixed
- ~~search_by_file missing GIN index~~ — fixed
- ~~parse_pg_vector_text string parsing overhead~~ — fixed by using pgvector crate
- ~~Hybrid search stop-words fallback~~ — fixed
- ~~sqlx missing uuid feature~~ — fixed
- ~~sqlx missing TLS feature~~ — fixed
- ~~Inconsistent HTTP error responses~~ — fixed
- ~~Pagination limit inconsistency~~ — fixed
- ~~save_memory ambiguous 422~~ — fixed
- ~~CLI hook.rs swallows JSON parse errors~~ — fixed
- ~~import_insights.rs regex truncation~~ — fixed
- ~~import_insights.rs title_exists swallows DB errors~~ — fixed
- ~~session_service hardcoded opencode binary~~ — fixed
- ~~session_service inconsistent empty-session handling~~ — fixed
- ~~build_tsquery empty string crash~~ — fixed
- ~~parse_json_value unnecessary clone~~ — fixed
- ~~get_all_pending_messages unfiltered~~ — fixed
- ~~get_queue_stats dead 'processed' count~~ — fixed
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
- ~~SearchService bypasses HybridSearch abstraction~~ — fixed, correctly delegates to internal struct
- ~~Queue processor UUID entropy & format violation~~ — fixed, uses strict UUIDv5 namespace over SHA-256 string input
- ~~ObservationType SPOT violation~~ — fixed, LLM prompt string dynamically generated from enum variants
- ~~Queueprocessor bottleneck~~ — fixed, background tasks spawned as fire-and-forget to avoid head-of-line blocking
- ~~Settings state ignored~~ — fixed, API updates propagated to `LlmClient::update_config` and `std::env::set_var`
- ~~Privacy leak in title~~ — fixed, applied `filter_private_content` inside `save_memory` title param
- ~~Privacy filter omission in queues~~ — fixed, `observe` and `observe_batch` handlers apply filter before database insertion
- ~~Infinite Memory LLM request bypasses retry logic~~ — already fixed (uses LlmClient::chat_completion with backoff)
- ~~Knowledge extraction skips updated observations~~ — already fixed (extracts from `save_result` regardless of is_new flag)
- ~~Dedup sweep overwrite bug~~ — already fixed (compute_merge resolves by noise_level importance)
- ~~Infinite memory time hierarchy violation~~ — already fixed (pipeline buckets strictly by 300s window)
