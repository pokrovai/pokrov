# Pokrov Runtime Config

`pokrov.example.yaml` describes the v1 runtime bootstrap configuration, including
sanitization profiles for `POST /v1/sanitize/evaluate` and LLM routing for
`POST /v1/chat/completions`.

## Required Sections

- `server.host`, `server.port`
- `logging.level`, `logging.format=json`
- `shutdown.drain_timeout_ms`, `shutdown.grace_period_ms`
- `security.api_keys[*].key` and `security.api_keys[*].profile`
- `sanitization.enabled`, `sanitization.default_profile`, `sanitization.profiles`
- `llm.providers`, `llm.routes`, `llm.defaults` (required when LLM proxy path is enabled)

Secrets must be provided only as references (`env:NAME` or `file:/path`).
`security.fail_on_unresolved_api_keys` is optional (default `false`) and enables
fail-fast startup when at least one configured key reference cannot be resolved.
`security.fail_on_unresolved_provider_keys` is optional (default `false`) and enables
fail-fast startup when at least one LLM provider auth reference cannot be resolved.

## Policy Profiles

- `minimal`: low-friction profile with `mask` defaults for secrets.
- `strict`: enforcement-first profile with `block`/`redact` defaults.
- `custom`: user-tunable profile with optional custom regex rules.

### Custom Rule Constraints

- `id` must be non-empty and unique inside a profile.
- `pattern` must compile as a regular expression.
- `action=replace` requires a `replacement` template.
- Empty matches are rejected unless `allow_empty_matches=true`.

## Local Run

```bash
export POKROV_API_KEY='dev-bootstrap-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

Probe check:

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
```

Evaluate check:

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/sanitize/evaluate \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "profile_id": "strict",
    "mode": "enforce",
    "payload": {
      "messages": [
        {"role": "user", "content": "token sk-test-12345678"}
      ]
    }
  }'
```

LLM proxy check:

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-4o-mini",
    "stream": false,
    "messages": [{"role": "user", "content": "hello"}]
  }' | jq
```

## LLM Routing Section

- `llm.providers[*].id` must be unique and referenced by `llm.routes[*].provider_id`.
- Provider secrets must use `env:` or `file:` references in `llm.providers[*].auth.api_key`.
- Only enabled providers and enabled routes are loaded into the runtime route table.
- `llm.defaults.profile_id` controls fallback profile selection when payload metadata
  does not specify a valid profile.
- `llm.routes[*].output_sanitization` overrides `llm.defaults.output_sanitization`
  per model route.
- `llm.defaults.stream_sanitization_max_buffer_bytes` limits buffered SSE body size
  when stream output sanitization is enabled (default `1048576` bytes).
- Known v1 limitation: sanitized stream responses are buffered up to this limit before
  forwarding to the client; oversized streams fail with upstream error.
