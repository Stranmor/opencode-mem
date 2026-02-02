# opencode-mem

Rust port of [claude-mem](https://github.com/thedotmack/claude-mem) for OpenCode.

**Status:** 🚧 In Development

## Architecture

```
opencode-mem/
├── crates/
│   ├── core/        # Domain types (Observation, Session, etc.)
│   ├── storage/     # SQLite + FTS5 + sqlite-vec
│   ├── embeddings/  # Vector embeddings (local models)
│   ├── search/      # Hybrid search (FTS + vector)
│   ├── llm/         # LLM compression (Antigravity API)
│   ├── http/        # HTTP API (Axum)
│   ├── mcp/         # MCP server
│   ├── plugin/      # OpenCode plugin hooks
│   └── cli/         # CLI binary
└── docs/
    ├── ADR.md       # Architecture decisions
    └── ROADMAP.md   # Feature roadmap
```

## Upstream Tracking

This project tracks changes from claude-mem:

```bash
git remote add upstream https://github.com/thedotmack/claude-mem.git
git fetch upstream
git log upstream/main --oneline -20  # See what's new
```

## Development

```bash
cargo build --workspace
cargo test --workspace
cargo run -p opencode-mem-cli
```

## License

MIT
