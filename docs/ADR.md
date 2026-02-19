# Architecture Decision Record

## ADR-001: Rust Instead of TypeScript

**Status:** Accepted

**Context:** claude-mem is written in TypeScript. We need to decide whether to port or adapt.

**Decision:** Full rewrite in Rust.

**Rationale:**
- RUST_ONLY doctrine (AGENTS.md)
- Better performance for embedding operations
- Single binary deployment
- No Node.js/Bun runtime dependency

**Consequences:**
- Cannot directly merge upstream changes
- Need manual porting of new features
- Benefit: smaller binary, faster startup

---

## ADR-002: sqlite-vec Instead of ChromaDB

**Status:** Superseded by ADR-002a

**Context:** claude-mem uses ChromaDB (Python) via MCP for vector search.

**Decision:** ~~Use sqlite-vec (SQLite extension) for vector storage.~~ Replaced by pgvector.

**Rationale:**
- ~~Single SQLite file (no external process)~~
- No Python dependency
- ~~Same query interface as FTS5~~
- ~~Rust bindings available~~

**Consequences:**
- Need to manage embedding model ourselves (candle/ort)
- ~~Simpler deployment (single binary + single db file)~~

---

## ADR-002a: pgvector Instead of sqlite-vec

**Status:** Accepted

**Context:** sqlite-vec has a 1024 record limit per vec0 table, no concurrent writers, and no streaming replication. PostgreSQL with pgvector eliminates all these constraints.

**Decision:** Use PostgreSQL with pgvector extension for vector storage. SQLite backend fully removed.

**Rationale:**
- No record limits (pgvector scales with PG)
- Concurrent readers and writers
- tsvector + GIN for full-text search (replaces FTS5)
- Streaming replication for backups
- Single database for all data (observations, embeddings, knowledge)

**Consequences:**
- Requires PostgreSQL server (not single-file deployment)
- pgvector extension must be installed
- Embedding dimensions: 1024 (BGE-M3)

---

## ADR-003: Local Embedding Model

**Status:** Superseded by ADR-003a

**Context:** Need embedding model for vector search.

**Decision:** ~~Use `all-MiniLM-L6-v2` (384 dim) via candle or onnxruntime.~~ Replaced by BGE-M3.

**Rationale:**
- Small model (~50MB)
- Fast inference (~10ms per embedding)
- Good quality for code/text similarity
- No API dependency

**Alternatives considered:**
- OpenAI API embeddings (rejected: API dependency, cost)
- Larger models (rejected: overkill for this use case)

---

## ADR-003a: Multilingual Embedding Model (BGE-M3)

**Status:** Accepted

**Context:** all-MiniLM-L6-v2 (384d) is English-only. Russian-language observations and knowledge entries produce low-quality embeddings, degrading semantic search for multilingual content.

**Decision:** Use `BGE-M3` (1024 dim, 8192 token context) via fastembed-rs/onnxruntime.

**Rationale:**
- 100+ languages including Russian — top scores on RusBEIR and ruMTEB benchmarks
- 1024 dimensions — higher fidelity vector representation
- 8192 token context — captures long observations without truncation
- Local inference, no API dependency
- fastembed-rs native support (`EmbeddingModel::BGEM3`)

**Tradeoffs:**
- Larger model (~1.1GB vs ~50MB) — acceptable, downloaded once and cached
- Slower inference (~50ms vs ~10ms) — acceptable for write-path embedding generation

**Migration:** PostgreSQL ALTER COLUMN from vector(384) to vector(1024). Existing embeddings must be regenerated via `backfill-embeddings` command.

---

## ADR-004: Hybrid Search Strategy

**Status:** Accepted

**Context:** Need to search memories effectively.

**Decision:** Hybrid search combining tsvector (PostgreSQL) + vector similarity (pgvector).

**Algorithm:**
1. tsvector keyword search → candidates
2. Vector similarity on candidates → re-rank
3. Merge scores: `0.3 * fts_score + 0.7 * vector_score`

**Rationale:**
- tsvector fast for keyword matches
- Vector search for semantic similarity
- Hybrid catches both exact and conceptual matches

---

## ADR-005: OpenCode Plugin Architecture

**Status:** Accepted

**Context:** Need to integrate with OpenCode.

**Decision:** TypeScript plugin that calls Rust HTTP API.

**Architecture:**
```
OpenCode → TS Plugin → HTTP :37777 → Rust Backend
```

**Plugin hooks used:**
- `experimental.chat.system.transform` — inject memories
- `experimental.chat.messages.transform` — enrich context
- `tool.execute.after` — capture observations
- `event` — session lifecycle

**Rationale:**
- OpenCode plugins must be TypeScript
- Heavy lifting in Rust (embeddings, search)
- Minimal TS code (just HTTP calls)

---

## ADR-006: 3-Layer Search Pattern

**Status:** Planned

**Context:** claude-mem uses 3-layer pattern for token efficiency.

**Decision:** Implement same pattern:

1. **Index** — `search(query)` returns IDs + titles only (~50-100 tokens/result)
2. **Timeline** — `timeline(anchor=ID)` returns context around result
3. **Full** — `get_observations([IDs])` returns complete data

**Rationale:**
- 10x token savings vs returning full data
- Agent filters first, then fetches details
- Matches claude-mem API for easier porting

---

## ADR-007: Configurable Low-Value Observation Filter

**Status:** Accepted

**Context:** The low-value observation filter was embedded in a single module with hardcoded patterns. We need configurable patterns without losing composite rules that require logic beyond simple string matching.

**Decision:** Extract the filter into a dedicated module with a static, env-configurable pattern set. Keep composite rules as code and expose a single public function that evaluates both composite rules and pattern-based matches.

**Rationale:**
- Single source of truth for low-value filtering
- Enables operational tuning via environment configuration
- Keeps complex matching logic explicit and testable

**Alternatives considered:**
- Keep everything hardcoded in one module (rejected: no runtime configurability)
- Load filter rules from external config file (rejected: adds IO surface and deployment complexity)

---

## ADR-008: Title-Based Observation Deduplication

**Status:** Accepted

**Context:** Identical observation titles are being stored multiple times across independent save paths.

**Decision:** Before saving an observation, check for an existing observation with the same title using
case-insensitive, trimmed comparison (`LOWER(TRIM(title))`). If a duplicate exists, skip the save
and log a debug message.

**Rationale:**
- Eliminates exact duplicates without adding new storage or similarity infrastructure
- Keeps behavior deterministic and easy to reason about
- Applies consistently across all observation save paths

**Alternatives considered:**
- FTS/embedding similarity (rejected: out of scope for exact-duplicate fix)
- Unique index on title (rejected: may change storage semantics and migrations)
