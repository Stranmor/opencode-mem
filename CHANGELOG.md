# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-03-03

### Added

- 17 MCP tools for persistent cross-session memory
- PostgreSQL + pgvector storage backend with full-text search (tsvector/GIN)
- Hybrid search combining FTS BM25 (50%) and vector similarity (50%)
- Semantic search via fastembed-rs (BGE-M3, 1024d, 100+ languages)
- Infinite Memory with hierarchical summaries and drill-down API
- Privacy filtering with `<private>` tag support across all storage paths
- Context-aware deduplication (CREATE/UPDATE/SKIP) to prevent observation drift
- 65 HTTP API endpoints with SSE real-time updates
- Web viewer with dark theme UI
- CLI with hooks for OpenCode integration (context, session-init, observe, summarize)
- Pending queue with crash recovery, visibility timeout, and dead letter queue
- Project exclusion filtering via `OPENCODE_MEM_EXCLUDED_PROJECTS`
- Configurable low-value observation filter (SPOT pattern)
- Knowledge base with global knowledge entries (skill, pattern, gotcha, architecture, tool_usage)
- Save memory tool for direct observation storage bypassing LLM compression