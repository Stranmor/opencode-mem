# Roadmap: claude-mem → opencode-mem

Full feature parity with [claude-mem](https://github.com/thedotmack/claude-mem).

## Phase 1: Core Infrastructure ✅ DONE

- [x] Cargo workspace structure
- [x] Core types (Observation, Session, SessionSummary)
- [x] PostgreSQL storage with migrations
- [x] tsvector + GIN full-text search

## Phase 2: Code Migration ✅ DONE

- [x] Migrate storage.rs → crates/storage (PostgreSQL-only)
- [x] Migrate llm.rs → crates/llm
- [x] Migrate http.rs → crates/http
- [x] Migrate mcp.rs → crates/mcp

## Phase 3: Session Management ✅ DONE

- [x] Dual session IDs (content_session_id, memory_session_id)
- [x] Session status tracking (active, completed, failed)
- [x] Prompt counter per session
- [x] User prompts storage (separate table)

## Phase 4: Session Summaries ✅ DONE

- [x] Structured summary (request, investigated, learned, completed, next_steps)
- [x] Auto-generate on session end (Stop hook)
- [x] FTS for summaries

## Phase 5: Vector Search (Embeddings) ✅ DONE

- [x] pgvector integration
- [x] Local embedding model (BGE-M3 via fastembed/ort, 1024d, multilingual)
- [x] Hybrid search (tsvector BM25 50% + vector cosine similarity 50%)

### Future (Vector Search)
- [ ] Granular sync (each field → separate embedding)

## Phase 6: 3-Layer Search Pattern ✅ DONE

- [x] Index layer (id, title, subtitle only — minimal tokens)
- [x] Timeline layer (anchor-based context retrieval)
- [x] Full layer (complete observation data)
- [x] `__IMPORTANT` tool (workflow documentation)

## Phase 7: Context Injection ✅ DONE

- [x] SessionStart hook → inject memories
- [x] Configurable observation count (total, full)
- [x] Interleaved timeline (observations + summaries)
- [x] Token economics display

## Phase 8: OpenCode Plugin

- [ ] `chat.message` hook (capture user prompts)
- [ ] `experimental.chat.system.transform` (inject context)
- [ ] `experimental.chat.messages.transform` (enrich messages)
- [ ] `tool.execute.after` (capture observations)
- [ ] `event` hook (session lifecycle)

## Phase 9: Privacy & Configuration ✅ DONE

- [x] `<private>` tag stripping
- [x] `<opencode-mem-context>` anti-recursion tags
- [x] settings.json configuration
- [x] Mode profiles (code, code--ru, etc.)

## Phase 10: Web UI ✅ DONE

- [x] Axum + htmx viewer
- [x] Real-time observation stream (SSE)
- [x] Session timeline view
- [x] Search interface

---

## Upstream Mapping

| claude-mem file | opencode-mem crate |
|-----------------|-------------------|
| `src/services/sqlite/Database.ts` | `crates/storage` |
| `src/services/sqlite/SessionStore.ts` | `crates/storage` |
| `src/services/sqlite/SessionSearch.ts` | `crates/search` |
| `src/services/sync/ChromaSync.ts` | `crates/embeddings` |
| `src/services/context/ContextBuilder.ts` | `crates/plugin` |
| `src/services/worker-service.ts` | `crates/http` |
| `src/servers/mcp-server.ts` | `crates/mcp` |
| `plugin/hooks/*` | `crates/plugin` |
