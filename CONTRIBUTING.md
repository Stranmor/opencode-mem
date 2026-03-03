# Contributing to opencode-mem

## Development Setup

1. **Rust toolchain** — install via [rustup](https://rustup.rs/) (stable channel)
2. **PostgreSQL 16+** with the [pgvector](https://github.com/pgvector/pgvector) extension
3. Create a database and set `DATABASE_URL`:
   ```bash
   createdb opencode_mem_dev
   psql -d opencode_mem_dev -c 'CREATE EXTENSION IF NOT EXISTS vector'
   export DATABASE_URL=postgres://localhost/opencode_mem_dev
   ```
4. Run migrations:
   ```bash
   cargo install sqlx-cli --no-default-features --features postgres
   sqlx database setup
   ```

## Building & Testing

```bash
cargo build --workspace
cargo test --workspace
```

Tests marked with `#[ignore]` require a live LLM API connection (Antigravity API) and are skipped by default. Run them explicitly:

```bash
cargo test --workspace -- --ignored
```

## Code Style

This project enforces strict linting:

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

All warnings are treated as errors. Fix clippy warnings before submitting.

## Pull Request Process

1. Fork the repository and create a feature branch
2. Make your changes with clear, atomic commits
3. Ensure `cargo fmt`, `cargo clippy`, and `cargo test` pass
4. Open a PR against `main` with a clear description of changes

## Architecture

See [AGENTS.md](AGENTS.md) for project architecture, crate layout, and design decisions.