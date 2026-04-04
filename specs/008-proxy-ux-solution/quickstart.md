# Quickstart: Proxy UX P0-P2 Improvements

## Preconditions

- Rust stable `1.85+`
- Runtime config with:
  - >=1 enabled provider for each planned transform profile (`openai_compatible`, `anthropic`, `gemini` as needed)
  - enabled routes with canonical names, aliases, and optional wildcard prefixes
  - optional fallback chains for retriable upstream failures
  - optional provider/model rate-limit profiles
- Gateway credential configured for Pokrov boundary access

## Example Config Fragment (Conceptual)

```yaml
llm:
  providers:
    - id: openai
      base_url: https://api.openai.com/v1
      upstream_path: /chat/completions
      transform_profile: openai_compatible
      auth: { api_key: env:OPENAI_API_KEY }
      enabled: true
    - id: anthropic
      base_url: https://api.anthropic.com/v1
      upstream_path: /messages
      transform_profile: anthropic
      auth: { api_key: env:ANTHROPIC_API_KEY }
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      aliases: [openai/gpt-4o-mini]
      wildcard_prefixes: [openai/]
      enabled: true
    - model: claude-sonnet-4-20250514
      provider_id: anthropic
      aliases: [anthropic/claude-sonnet-4]
      fallback_chain_id: llm-fallback-main
      enabled: true
  fallback_chains:
    - id: llm-fallback-main
      trigger_policy: transport_or_5xx
      steps:
        - route_model: gpt-4o-mini
          max_attempts: 1

responses:
  passthrough:
    enabled: true
    allow_subset_fallback: false

response_envelope:
  pokrov_metadata:
    mode: suppressed
```

## Local Run

```bash
export POKROV_API_KEY='gateway-dev-key'
export OPENAI_API_KEY='provider-dev-key'
export ANTHROPIC_API_KEY='provider-dev-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

## Verification Scenarios

### 1) Model Discovery (`/v1/models`)

```bash
curl -sS http://127.0.0.1:8080/v1/models \
  -H "X-Pokrov-Api-Key: $POKROV_API_KEY" | jq
```

Expected:
- `object: list`, non-empty `data`
- includes canonical + alias (+ wildcard-derived entries when applicable)
- excludes disabled providers/routes

### 2) Alias and Wildcard Routing

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "X-Pokrov-Api-Key: $POKROV_API_KEY" \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"OPENAI/GPT-4O-MINI","messages":[{"role":"user","content":"hello"}]}' | jq
```

Expected:
- deterministic resolution (`exact/alias/wildcard` order preserved)
- metadata-only logs and stable request contract

### 3) Fallback on Retriable Upstream Failure

1. Configure primary route to return upstream transport/5xx error.
2. Keep fallback chain enabled.

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "X-Pokrov-Api-Key: $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"anthropic/claude-sonnet-4","messages":[{"role":"user","content":"hello"}]}' | jq
```

Expected:
- fallback step activated only for eligible retriable failure
- if fallback succeeds: `200`
- if exhausted: deterministic metadata-only `fallback_exhausted` error

### 4) `/v1/responses` Native Passthrough

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/responses \
  -H "X-Pokrov-Api-Key: $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"gpt-4o-mini","input":"Say hello"}' | jq
```

Expected:
- no forced downgrade to chat-completions subset when passthrough mode enabled
- errors remain predictable and metadata-only

### 5) Provider/Model Rate Limit Behavior

Send repeated requests exceeding configured provider/model budget.

Expected:
- deterministic `429 rate_limit_exceeded`
- no raw payload leakage
- observability counters for provider/model budget events increment

## Minimal Validation Command Set

```bash
cargo test --test contract -- llm_proxy_api_contract
cargo test --test contract -- responses_api_contract
cargo test --test integration -- llm_proxy_routing_path
cargo test --test integration -- llm_proxy_upstream_error_path
cargo test --test integration -- llm_proxy_chat_completions_regression
cargo test --test security -- llm_proxy_metadata_leakage
cargo test --test security -- responses_metadata_leakage
cargo test --test performance -- llm_proxy_overhead_budget
cargo test --test performance -- responses_proxy_overhead_budget
```

## Acceptance Evidence to Collect

- Contract evidence for `/v1/models`, `/v1/chat/completions`, `/v1/responses` with updated error matrix.
- Integration evidence for alias/wildcard resolution, fallback success/exhaustion, passthrough behavior.
- Security evidence for metadata-only logs/errors across transform and fallback paths.
- Performance evidence for overhead stability with wildcard/fallback/transform and provider/model limits.
