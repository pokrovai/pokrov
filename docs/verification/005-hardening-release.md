# Hardening Release Verification (005)

## Scope

- Deterministic dual-bucket rate limiting (`requests` + `token_units`).
- Mandatory Prometheus metrics exposition on `/metrics`.
- Metadata-only logging and audit output.
- Release evidence generation and schema validation.

## Runtime Start

```bash
export POKROV_API_KEY='pilot-hardening-key'
export OPENAI_API_KEY='provider-hardening-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

## Probe And Metrics Checks

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
curl -sS http://127.0.0.1:8080/metrics | rg 'pokrov_'
```

Expected result:

- `/health` returns `200`.
- `/ready` returns `200` while runtime is healthy.
- `/metrics` output contains:
  - `pokrov_requests_total`
  - `pokrov_blocked_total`
  - `pokrov_rate_limit_events_total`
  - `pokrov_upstream_errors_total`
  - `pokrov_request_duration_seconds`

## Rate-Limit Verification

```bash
for i in {1..200}; do
  curl -sS -i \
    -H "Authorization: Bearer $POKROV_API_KEY" \
    -H 'Content-Type: application/json' \
    -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"rate-limit test"}]}' \
    http://127.0.0.1:8080/v1/chat/completions >/tmp/pokrov-rl-$i.txt
done
```

Expected result:

- Some responses return `429`.
- `429` body includes only metadata-safe fields:
  - `request_id`
  - `error.code=rate_limit_exceeded`
  - `retry_after_ms`
  - `limit`
  - `remaining`
  - `reset_at`
- `429` headers include:
  - `Retry-After`
  - `X-RateLimit-Limit`
  - `X-RateLimit-Remaining`
  - `X-RateLimit-Reset`

## Logging Safety Check

- Run both allow and blocked scenarios through LLM and MCP paths.
- Verify logs contain only allowlisted metadata (`request_id`, `decision`, `status_code`, `duration_ms`).
- Verify logs do not contain raw prompts, MCP arguments, or secret tokens.

## Verification Matrix

- `cargo test`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features`

All commands must pass before hardening release evidence is marked as `gate_status=pass`.

## Release Evidence Scaffold

```bash
cargo run -p pokrov-runtime -- \
  --release-evidence-output ./config/release/release-evidence.json \
  --release-id hardening-v1 \
  --artifact ./config/pokrov.example.yaml \
  --artifact ./config/release/verification-checklist.md
```
