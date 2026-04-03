# Pokrov Runtime Config

`pokrov.example.yaml` describes the v1 runtime bootstrap configuration, including
sanitization profiles for `POST /v1/sanitize/evaluate`.

## Required Sections

- `server.host`, `server.port`
- `logging.level`, `logging.format=json`
- `shutdown.drain_timeout_ms`, `shutdown.grace_period_ms`
- `security.api_keys[*].key` and `security.api_keys[*].profile`
- `sanitization.enabled`, `sanitization.default_profile`, `sanitization.profiles`

Secrets must be provided only as references (`env:NAME` or `file:/path`).

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
