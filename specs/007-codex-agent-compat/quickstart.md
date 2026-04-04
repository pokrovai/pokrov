# Quickstart: Codex Compatibility (`/v1/responses`)

## Preconditions

- Rust stable `1.85+`
- Runtime config with LLM provider route for target model
- Gateway credential for Pokrov boundary access
- Upstream provider credential for passthrough scenario
- `auth.upstream_auth_mode` explicitly configured (`static` or `passthrough`)

## Example Config Fragment (Conceptual)

```yaml
auth:
  upstream_auth_mode: passthrough

identity:
  resolution_order:
    - gateway_auth_subject
    - x_pokrov_client_id

llm:
  providers:
    - id: openai
      base_url: https://api.openai.com/v1
      auth:
        api_key: env:OPENAI_API_KEY
```

Notes:

- `POST /v1/responses` is a minimal Codex compatibility endpoint for v1.
- `POST /v1/chat/completions` remains supported and unchanged.
- In passthrough mode, use split auth headers:
  - gateway: `X-Pokrov-Api-Key`
  - upstream: `Authorization: Bearer ...`

## Local Run

```bash
export POKROV_API_KEY='gateway-dev-key'
export OPENAI_API_KEY='provider-dev-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

## Verification Scenarios

### 1) Responses Non-Stream Happy Path

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/responses \
  -H "X-Pokrov-Api-Key: $POKROV_API_KEY" \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-4o-mini",
    "stream": false,
    "input": "print hello world in rust"
  }' | jq
```

Expected:

- Structured JSON response with `request_id`.
- `pokrov` metadata present.
- No raw sensitive payload in logs/audit.

### 2) Responses Stream Happy Path

```bash
curl -N -X POST http://127.0.0.1:8080/v1/responses \
  -H "X-Pokrov-Api-Key: $POKROV_API_KEY" \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-4o-mini",
    "stream": true,
    "input": "explain ownership in rust"
  }'
```

Expected:

- SSE-compatible stream.
- Stream sanitization preserved.
- Terminal `data: [DONE]` frame present.

### 3) Passthrough Missing Upstream Credential Block Path

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/responses \
  -H "X-Pokrov-Api-Key: $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-4o-mini",
    "stream": false,
    "input": "hello"
  }' | jq
```

Expected:

- Structured metadata-only `422` error (`upstream_credential_missing`).
- No upstream call performed.

### 4) Gateway Auth Failure Path

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/responses \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-4o-mini",
    "stream": false,
    "input": "hello"
  }' | jq
```

Expected:

- Structured metadata-only `401` error (`gateway_unauthorized`).
- Request blocked before upstream.

### 5) Backward Compatibility Check

1. Execute existing `POST /v1/chat/completions` happy path.
2. Verify response contract and behavior unchanged.

Expected:

- Existing chat-completions tests remain green.

## Logging and Audit Safety Checks

1. Run allow/block/upstream-failure scenarios for `/v1/responses`.
2. Inspect logs, audit events, and metrics output.

Expected fields:

- Present: `request_id`, route, decision, auth stage outcome, status class.
- Absent: raw prompt/input, raw output chunks, gateway key, upstream bearer token.

## Minimal Validation Command Set

```bash
cargo test --test contract -- llm_proxy_api_contract
cargo test --test integration -- llm
cargo test --test security -- metadata
cargo test --test performance -- overhead
```

## Acceptance Evidence to Collect

- Contract evidence for `/v1/responses` sync and stream flows.
- Security evidence for split-auth block paths and metadata-only guarantees.
- Regression evidence for unchanged `/v1/chat/completions` behavior.
- Metrics/log excerpts proving observability for the new route.
