# Contributing

## Scope and Principles

Pokrov.AI is a production-oriented security proxy.
Contributions should preserve these invariants:

- sanitization-first request/response handling;
- metadata-only audit and logging (no raw payload leaks);
- deterministic policy behavior (`allow`, `mask`, `redact`, `block`);
- no implicit contract changes across public APIs and config.

## Development Setup

1. Install Rust stable toolchain.
2. Clone the repository.
3. Use the example config from `config/pokrov.example.yaml`.

## Local Validation

Run before opening a pull request:

```bash
cargo check --workspace
cargo fmt --check
cargo test --workspace
cargo clippy --all-targets --all-features
```

## Pull Request Guidance

- Keep diffs minimal and focused.
- Explain what changed and why.
- Include tests for behavior changes.
- Avoid unrelated refactors in the same PR.
- Do not introduce new dependencies unless strictly necessary.

## Documentation

Update `README.md` and/or `docs/configuration.md` when behavior, endpoints, or configuration semantics change.

## Security-Sensitive Changes

For changes touching sanitization, auth, audit, or policy flow:

- call out potential leakage or bypass risks;
- describe mitigation in the PR text;
- include regression tests where possible.
