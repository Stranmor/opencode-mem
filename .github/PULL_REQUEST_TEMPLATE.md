## What changed

Describe the user-visible behavior and the affected MCP, HTTP, CLI, storage, or background-processing boundary.

## Verification

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Recovery and degraded-mode behavior was tested when applicable

## Data safety

- [ ] No database URL, provider key, private observation, user identifier, or local private path is included.
- [ ] Migrations and persistence changes preserve existing data or document an explicit recovery path.
