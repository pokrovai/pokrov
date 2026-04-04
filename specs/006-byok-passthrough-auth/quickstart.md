# Quickstart: BYOK Passthrough Auth

## Preconditions

- Rust stable `1.85+`
- Valid runtime config with LLM and/or MCP paths enabled
- At least one gateway credential for Pokrov boundary access
- At least one client identity mapping for policy/rate-limit profiles
- Access to upstream provider account for BYOK testing

## Example Config Fragment (Conceptual)

```yaml
security:
  api_keys:
    - key: env:POKROV_GATEWAY_KEY
      profile: strict

auth:
  upstream_auth_mode: passthrough

identity:
  resolution_order:
    - gateway_auth_subject
    - x_pokrov_client_id
  required_for_policy: false
  required_for_rate_limit: false

llm:
  providers:
    - id: openai
      auth:
        api_key: env:OPENAI_API_KEY
```

Notes:

- `upstream_auth_mode=static`: upstream credentials are taken from config.
- `upstream_auth_mode=passthrough`: upstream credentials are taken from client request context.
- Gateway auth to Pokrov remains independent from upstream provider credentials.

## Local Run

```bash
export POKROV_GATEWAY_KEY='gateway-dev-key'
export OPENAI_API_KEY='provider-static-dev-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

## Verification Scenarios

### 1) Static Mode Backward Compatibility

1. Set mode to `static`.
2. Send `POST /v1/chat/completions` with valid gateway auth.
3. Verify request succeeds with configured provider key.

Expected:

- Existing static deployment behavior remains unchanged.

### 2) Passthrough Happy Path

1. Set mode to `passthrough`.
2. Send request with valid gateway auth in `X-Pokrov-Api-Key` and provider credential in `Authorization`.
3. Verify response is returned from upstream.

Expected:

- Request is proxied successfully.
- Sanitization/policy checks are still applied before upstream call.

### 3) Passthrough Block Path (Missing Provider Credential)

1. Keep mode `passthrough`.
2. Send request with valid gateway auth in `X-Pokrov-Api-Key` but without provider credential.

Expected:

- Structured metadata-only `422` error.
- No upstream call executed.

### 4) Gateway Auth Failure Path

1. Send request with invalid or missing gateway auth.
2. Include any provider credential.

Expected:

- Structured metadata-only `401` error at Pokrov boundary.
- No upstream call executed.

### 5) Identity-Based Isolation Check

1. Send equal traffic from two different client identities.
2. Make one identity exceed its rate-limit budget.

Expected:

- The over-limit identity is blocked by its own budget.
- The second identity continues within its own budget unaffected.

## Logging and Audit Safety Checks

1. Execute allow/block scenarios for both LLM and MCP paths.
2. Inspect logs/audit records.

Expected:

- Present: `request_id`, `auth_mode`, `decision`, `status_code`, `policy_profile`.
- Absent: raw provider keys, raw gateway keys, raw `Authorization` header values, raw prompt/tool payload fragments.

## Minimal Validation Command Set

```bash
cargo test --test integration -- byok
cargo test --test security -- auth
cargo test --test security -- metadata
cargo test --test performance -- overhead
```

## Acceptance Evidence to Collect

- Happy-path evidence for `static` and `passthrough`.
- Block-path evidence for missing gateway auth and missing upstream credentials.
- Evidence that policy/rate-limit decisions are identity-bound.
- Evidence that logs and audit remain metadata-only.
