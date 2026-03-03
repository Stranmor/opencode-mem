# Contributing to opencode-mem

Rust workspace with 10 crates. PostgreSQL + pgvector backend.

## Development Setup
Prerequisites include Rust 1.75+ and a running PostgreSQL instance with the pgvector extension.
- Clone the repository to your local machine.
- Run `cargo build --workspace` to compile the project.

## Running Tests
Tests are executed using standard Cargo commands.
- Run `cargo test --workspace` to execute unit tests.
- Integration tests require the `DATABASE_URL` environment variable pointing to a PostgreSQL instance with the pgvector extension.

## Code Quality
All code must pass strict formatting and linting checks before submission. All warnings are treated as errors.
- Format code: `cargo fmt --all`
- Run lints: `cargo clippy --workspace -- -D warnings`

## Pull Request Process
- Fork the repository and create a dedicated feature branch.
- Implement your changes, run tests, and ensure lints pass.
- Open a Pull Request against the `main` branch.
- Include a clear description of the changes and the problem they solve.

## Reporting Issues
Use GitHub Issues to report bugs or request features. When filing a bug report, include:
- The output of `rustc --version`
- Your operating system details
- Minimal reproduction steps

## License
All contributions to this repository are licensed under the MIT License.