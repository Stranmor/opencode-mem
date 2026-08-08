# Repository instructions

`opencode-mem` is a public Rust workspace for a PostgreSQL-backed MCP memory
server. Keep repository instructions short, durable, and limited to constraints
that affect contributors and automated coding agents.

## Invariants

- PostgreSQL and pgvector are the canonical persistence and vector-search path.
- Preserve `<private>` filtering at every ingest boundary before persistence,
  queues, embeddings, summaries, logs, or external model calls.
- MCP stdio must emit protocol messages only; diagnostics belong on stderr.
- Database migrations are forward-only and must preserve existing data.
- Invalid or unavailable state must remain explicit; do not fabricate fallback
  observations, scores, identifiers, or successful results.

## Repository hygiene

- Keep README capability and readiness claims aligned with current CI and a
  reproducible consumer path.
- Never commit credentials, private endpoints, machine-specific paths, raw
  production data, generated diffs, command output, or one-off scratch files.
- Add or update focused tests when behavior, parsing, persistence, privacy, or
  recovery semantics change.
- Preserve unrelated work and avoid history rewrites on shared branches.

## Verification

Before committing code changes, run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace --release
cargo +1.88.0 check --workspace
```

Ignored integration tests require their documented PostgreSQL or provider
dependencies and must be reported separately from the default test result.
