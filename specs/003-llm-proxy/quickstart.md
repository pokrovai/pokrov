# Quickstart: LLM Proxy

## Preconditions

- Rust stable `1.85+`
- Valid Pokrov YAML config with `security.api_keys`, `sanitization.profiles`, `llm.providers`, `llm.routes`, and `llm.defaults`
- Provider API keys configured through `env:` or `file:` references
- `jq` and `curl` installed

## Example Config Fragment

```yaml
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict

sanitization:
  enabled: true
  default_profile: strict
  profiles:
    strict:
      mode_default: enforce
      categories:
        secrets: block
        pii: redact
        corporate_markers: mask
      mask_visible_suffix: 4

llm:
  providers:
    - id: openai
      base_url: https://api.openai.com/v1
      auth:
        api_key: env:OPENAI_API_KEY
      timeout_ms: 30000
      retry_budget: 1
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      output_sanitization: true
      enabled: true
  defaults:
    profile_id: strict
    output_sanitization: true
```

## Local Run

```bash
export POKROV_API_KEY='llm-proxy-test-key'
export OPENAI_API_KEY='provider-test-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

Probe checks:

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
```

## Non-Stream Happy Path Verification

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-4o-mini",
    "stream": false,
    "messages": [
      {"role": "user", "content": "Summarize this: token sk-test-123"}
    ],
    "metadata": {"profile": "minimal"}
  }' | jq
```

Validate:

- `request_id` is present in body and `X-Request-Id` header.
- `pokrov.profile`, `pokrov.action`, and `pokrov.rule_hits` are present.
- Sanitization happens before upstream forwarding.

## Block Path Verification

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-4o-mini",
    "stream": false,
    "messages": [
      {"role": "user", "content": "token sk-test-12345678"}
    ]
  }' | jq
```

Validate:

- HTTP `403`
- `error.code=policy_blocked`
- No upstream provider call is executed

## Streaming Verification

```bash
curl -N -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-4o-mini",
    "stream": true,
    "messages": [
      {"role": "user", "content": "Generate a short plan"}
    ]
  }'
```

Validate:

- Stream uses OpenAI-style SSE framing (`data: ...` and terminal `data: [DONE]`).
- `Content-Type: text/event-stream` is returned.
- With `output_sanitization=true`, sensitive output fragments are sanitized.

## Upstream Error Verification

Temporarily point provider to an unavailable host and repeat the request.

Validate:

- HTTP `503` for unavailable upstream.
- `error.code=upstream_unavailable` and `provider_id` are present.
- Error response contains metadata-only fields.

## Automated Verification

```bash
cargo check --workspace
cargo test --workspace
```

## Expected Evidence

- Contract tests validate OpenAI-compatible endpoint and metadata contract.
- Integration tests validate happy path, block path, routing, stream, output sanitization, and upstream failure behavior.
- Security tests validate auth rejection and metadata-only response safety.
- Performance tests validate non-stream LLM overhead budget (`p95 <= 50ms`, `p99 <= 100ms`).
