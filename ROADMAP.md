# Roadmap: claude-mem → opencode-mem

Full feature parity with [claude-mem](https://github.com/thedotmack/claude-mem).

## Phase 1: Core Infrastructure ✅ DONE

- [x] Cargo workspace structure
- [x] Core types (Observation, Session, SessionSummary)
- [x] SQLite storage with migrations
- [x] FTS5 full-text search

## Phase 2: Existing Code Migration 🔄 IN PROGRESS

- [ ] Migrate storage.rs → crates/storage
- [ ] Migrate llm.rs → crates/llm  
- [ ] Migrate http.rs → crates/http
- [ ] Migrate mcp.rs → crates/mcp

## Phase 3: Session Management

- [ ] Dual session IDs (content_session_id, memory_session_id)
- [ ] Session status tracking (active, completed, failed)
- [ ] Prompt counter per session
- [ ] User prompts storage (separate table)

## Phase 4: Session Summaries

- [ ] Structured summary (request, investigated, learned, completed, next_steps)
- [ ] Auto-generate on session end (Stop hook)
- [ ] FTS5 for summaries

## Phase 5: Vector Search (Embeddings)

- [ ] sqlite-vec integration
- [ ] Local embedding model (all-MiniLM-L6-v2 via candle/ort)
- [ ] Hybrid search (FTS5 + vector similarity)
- [ ] Granular sync (each field → separate embedding)

## Phase 6: 3-Layer Search Pattern

- [ ] Index layer (id, title, subtitle only — minimal tokens)
- [ ] Timeline layer (anchor-based context retrieval)
- [ ] Full layer (complete observation data)
- [ ] `__IMPORTANT` tool (workflow documentation)

## Phase 7: Context Injection

- [ ] SessionStart hook → inject memories
- [ ] Configurable observation count (total, full)
- [ ] Interleaved timeline (observations + summaries)
- [ ] Token economics display

## Phase 8: OpenCode Plugin

- [ ] `chat.message` hook (capture user prompts)
- [ ] `experimental.chat.system.transform` (inject context)
- [ ] `experimental.chat.messages.transform` (enrich messages)
- [ ] `tool.execute.after` (capture observations)
- [ ] `event` hook (session lifecycle)

## Phase 9: Privacy & Configuration

- [ ] `<private>` tag stripping
- [ ] `<opencode-mem-context>` anti-recursion tags
- [ ] settings.json configuration
- [ ] Mode profiles (code, code--ru, etc.)

## Phase 10: Web UI

- [ ] Axum + htmx viewer
- [ ] Real-time observation stream (SSE)
- [ ] Session timeline view
- [ ] Search interface

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
