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

**Status:** Accepted

**Context:** claude-mem uses ChromaDB (Python) via MCP for vector search.

**Decision:** Use sqlite-vec (SQLite extension) for vector storage.

**Rationale:**
- Single SQLite file (no external process)
- No Python dependency
- Same query interface as FTS5
- Rust bindings available

**Consequences:**
- Need to manage embedding model ourselves (candle/ort)
- Simpler deployment (single binary + single db file)

---

## ADR-003: Local Embedding Model

**Status:** Accepted

**Context:** Need embedding model for vector search.

**Decision:** Use `all-MiniLM-L6-v2` (384 dim) via candle or onnxruntime.

**Rationale:**
- Small model (~50MB)
- Fast inference (~10ms per embedding)
- Good quality for code/text similarity
- No API dependency

**Alternatives considered:**
- OpenAI API embeddings (rejected: API dependency, cost)
- Larger models (rejected: overkill for this use case)

---

## ADR-004: Hybrid Search Strategy

**Status:** Accepted

**Context:** Need to search memories effectively.

**Decision:** Hybrid search combining FTS5 + vector similarity.

**Algorithm:**
1. FTS5 keyword search → candidates
2. Vector similarity on candidates → re-rank
3. Merge scores: `0.3 * fts_score + 0.7 * vector_score`

**Rationale:**
- FTS5 fast for keyword matches
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
