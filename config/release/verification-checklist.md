# Hardening Release Verification Checklist

- [ ] `cargo test` passed on release commit.
- [ ] `cargo fmt --check` passed on release commit.
- [ ] `cargo clippy --all-targets --all-features` passed on release commit.
- [ ] `/health`, `/ready`, and `/metrics` probes validated in release environment.
- [ ] Rate-limit burst checks returned predictable `429 rate_limit_exceeded` responses.
- [ ] Structured logs verified as metadata-only (no raw payloads or secret leakage).
- [ ] Performance evidence captured (`p95`, `p99`, throughput, startup).
- [ ] Security evidence captured (invalid-auth, abuse, log-safety, secret-handling).
- [ ] Operational evidence captured (metrics coverage, readiness, graceful shutdown).
- [ ] `release-evidence.json` generated and attached to the release bundle.
- [ ] Deterministic recognizer startup validation evidence captured (success + fail-fast path).
- [ ] Deterministic replay identity evidence captured for repeated identical payloads.
- [ ] Deterministic explain/audit reason-code evidence captured with metadata-only safety proof.

## Deterministic Recognizer Evidence Log

- 2026-04-05: `cargo test` passed after deterministic recognizer changes (contract/integration/security/performance suites green).
- 2026-04-05: `cargo fmt --check` executed and failed due pre-existing formatting drift across unrelated modules.
- 2026-04-05: `cargo clippy --all-targets --all-features` executed and failed with mixed-toolchain artifact mismatch (`E0514`) in this environment.
- 2026-04-05: Startup success/fail-fast deterministic config paths validated in `tests/integration/startup_config_flow.rs`.
- 2026-04-05: Replay identity stability validated in `tests/contract/sanitization_evaluate_contract.rs`.
- 2026-04-05: Metadata-only explain/audit deterministic reason-code safety validated in integration and security suites.
