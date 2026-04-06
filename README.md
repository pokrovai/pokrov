# Pokrov Runtime

Pokrov.AI v1 is a self-hosted, security-first proxy for a safe `AI agent -> LLM/MCP -> AI agent` flow.
The service receives agent traffic, applies sanitization and validation policies before any upstream call, and returns only safe results.

## Purpose

Pokrov is a single control point for coding/AI agents to:

- prevent leakage of secrets, PII, and corporate markers to LLM/MCP systems;
- enforce centralized `allow/mask/redact/block` policies;
- separate `enforce` and `dry_run` modes for safe rollout;
- keep metadata-only audit records (no raw payloads).

## Implemented in v1

- Sanitization core with recursive JSON processing (`POST /v1/sanitize/evaluate`).
- LLM proxy path (OpenAI-compatible `chat/completions`) with:
  - input sanitization,
  - optional output sanitization,
  - stream and non-stream handling.
- MCP mediation path with:
  - server/tool allowlists,
  - blocking and argument validation,
  - tool output sanitization.
- API key auth with policy-profile binding.
- BYOK upstream auth mode (`static` and `passthrough`) with support for
  OpenAI-compatible single-bearer passthrough on LLM endpoints.
- Rate limiting (requests + token units) with `enforce` and `dry_run` modes.
- Health/readiness/metrics endpoints.
- Metadata-only audit and structured logging.
- Release evidence artifact generation.
- Optional NER-based PII detection (persons, organizations) for English and Russian
  with per-profile entity type filtering and ONNX inference.

## Runtime Endpoints

- `GET /health`
- `GET /ready`
- `GET /metrics`
- `POST /v1/sanitize/evaluate`
- `GET /v1/models`
- `POST /v1/chat/completions`
- `POST /v1/responses`
- `POST /v1/mcp/tool-call`
- `POST /v1/mcp/tools/{toolName}/invoke`

## Out of Scope for v1

Per `docs/PRD.md`, v1 intentionally excludes: A2A proxy, RBAC/IAM, SIEM export, web UI/admin panel, heavy ML NER (beyond lightweight ONNX models), response caching, and a full control plane.

## Quick Start (Local)

```bash
export POKROV_API_KEY='dev-runtime-key'
export OPENAI_API_KEY='provider-dev-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

With NER enabled:

```bash
cargo run -p pokrov-runtime --features ner -- --config ./config/pokrov.example.yaml
```

Probe checks:

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
curl -sS http://127.0.0.1:8080/metrics | rg 'pokrov_'
```

Sanitization check:

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
  }' | jq
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

Model discovery check:

```bash
curl -sS http://127.0.0.1:8080/v1/models \
  -H "Authorization: Bearer $POKROV_API_KEY" | jq
```

BYOK passthrough check:

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "X-Pokrov-Api-Key: $POKROV_API_KEY" \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-4o-mini",
    "stream": false,
    "messages": [{"role": "user", "content": "hello"}]
  }' | jq
```

## NER (Named Entity Recognition)

Pokrov includes an optional NER module that detects **person names** and **organization names**
in English and Russian text using lightweight ONNX models. NER hits are merged with
builtin/deterministic detectors and go through the same overlap resolution and policy pipeline.

### How It Works

1. The sanitization engine extracts string leaves from the JSON payload.
2. Candidate strings are filtered (dates, numbers, short lowercase tokens are skipped).
3. Unique strings are batched and sent to the NER engine for inference.
4. Detected entities are mapped to `pii` (person) or `corporate_markers` (organization) categories.
5. Per-profile `entity_types` controls which entity kinds each profile detects.

### Prerequisites

```bash
pip install torch transformers optimum
```

### Download Models

Download both recommended models:

```bash
./scripts/download-ner-model.sh --all
```

Or download a single model:

```bash
./scripts/download-ner-model.sh dslim/bert-base-NER models/bert-base-NER
./scripts/download-ner-model.sh cointegrated/rubert-tiny2-ner models/ner-rubert-tiny-news
```

Default model locations:

| Language | Model | Directory |
|----------|-------|-----------|
| EN | `dslim/bert-base-NER` | `models/bert-base-NER/` |
| RU | `cointegrated/rubert-tiny2-ner` | `models/ner-rubert-tiny-news/` |

### Configuration

Enable NER in the runtime config under the top-level `ner` section and set `ner_enabled: true`
on each sanitization profile that should use NER detection.

```yaml
ner:
  enabled: true
  models:
    - language: en
      model_path: "./models/bert-base-NER/model.onnx"
      tokenizer_path: "./models/bert-base-NER/tokenizer.json"
      priority: 100
    - language: ru
      model_path: "./models/ner-rubert-tiny-news/model.onnx"
      tokenizer_path: "./models/ner-rubert-tiny-news/tokenizer.json"
      priority: 100
  fallback_language: en
  timeout_ms: 80
  confidence_threshold: 0.7
  max_seq_length: 512
  profiles:
    strict:
      fail_mode: fail_closed
      entity_types: [person, organization]
    minimal:
      fail_mode: fail_open
      entity_types: [person]

sanitization:
  profiles:
    strict:
      ner_enabled: true
      # ...
    minimal:
      ner_enabled: true
      # ...
```

**Key settings:**

| Field | Description |
|-------|-------------|
| `ner.enabled` | Global NER on/off switch |
| `ner.models[].language` | Language tag (`en`, `ru`, etc.) used for auto-detection |
| `ner.models[].priority` | Model priority when multiple models match the same language |
| `ner.profiles.<name>.entity_types` | Which entity types to detect for this profile: `person`, `organization` |
| `ner.profiles.<name>.fail_mode` | `fail_open` (skip NER on error) or `fail_closed` (block request on error) |
| `sanitization.profiles.<name>.ner_enabled` | Per-profile toggle — must be `true` for NER to run on that profile |

### Running with NER

```bash
export POKROV_API_KEY='dev-runtime-key'
cargo run -p pokrov-runtime --features ner -- --config ./config/pokrov.example.yaml
```

### NER Examples

**English — person and organization detection:**

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/sanitize/evaluate \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "profile_id": "strict",
    "mode": "enforce",
    "effective_language": "en",
    "payload": {
      "text": "John Smith works at Google and Alice Johnson visited Microsoft"
    }
  }' | jq '{action: .final_action, hits: .explain.rule_hits_total, ner: .explain.family_counts.ner, sanitized: .sanitized_payload}'
```

Result:

```json
{
  "action": "redact",
  "hits": 4,
  "ner": 4,
  "sanitized": {
    "text": "[REDACTED] works at [REDACTED] and [REDACTED] visited [REDACTED]"
  }
}
```

**Russian — person and organization detection:**

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/sanitize/evaluate \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "profile_id": "strict",
    "mode": "enforce",
    "effective_language": "ru",
    "payload": {
      "text": "Иван Петров работает в Газпром и Мария Иванова посетила офис Яндекса"
    }
  }' | jq '{action: .final_action, hits: .explain.rule_hits_total, ner: .explain.family_counts.ner, sanitized: .sanitized_payload}'
```

Result:

```json
{
  "action": "redact",
  "hits": 4,
  "ner": 4,
  "sanitized": {
    "text": "[REDACTED] работает в [REDACTED] и [REDACTED] посетила офис [REDACTED]"
  }
}
```

**Mixed payload — NER combined with builtin secret detection:**

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/sanitize/evaluate \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "profile_id": "strict",
    "mode": "enforce",
    "effective_language": "en",
    "payload": {
      "user_name": "John Smith",
      "company": "Google",
      "secret_key": "sk-test-abc12345",
      "message": "Alice Johnson from Microsoft confirmed the payment"
    }
  }' | jq '{action: .final_action, hits: .explain.rule_hits_total, families: .explain.family_counts}'
```

Result — NER hits merged with builtin secret/card detection:

```json
{
  "action": "block",
  "hits": 7,
  "families": {
    "builtin": 3,
    "ner": 4,
    "normalized_hit": 7,
    "resolved_hit": 7
  }
}
```

### Per-Profile Entity Type Filtering

The `ner.profiles` section controls which entity types each sanitization profile detects:

- **strict** profile with `entity_types: [person, organization]` — detects both persons and organizations.
- **minimal** profile with `entity_types: [person]` — detects only person names; organization names pass through.
- Profiles not listed in `ner.profiles` use the adapter default (`person` + `organization`).

### Performance

- EN model (`dslim/bert-base-NER`): ~20-50ms per batch, PER F1=0.91.
- RU model (`cointegrated/rubert-tiny2-ner`): ~15-30ms per batch.
- Batch inference with automatic text deduplication — identical strings across JSON fields
  are inferred only once.
- P95 sanitization + NER overhead stays within the 50ms latency budget.

## Container Run

```bash
docker build -t pokrov-runtime:latest .
docker run --rm -p 8080:8080 \
  -e POKROV_API_KEY='dev-runtime-key' \
  -e OPENAI_API_KEY='provider-dev-key' \
  pokrov-runtime:latest
```

## OpenCode Setup with Pokrov

You can connect Pokrov in OpenCode as a custom OpenAI-compatible provider.

1. Add credentials for your provider id (for example, `pokrov`) via `/connect` (or `opencode auth login`).
2. Add or update `opencode.json` in your project:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "pokrov": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "Pokrov Runtime",
      "options": {
        "baseURL": "http://127.0.0.1:8080/v1"
      },
      "models": {
        "gpt-4o-mini": {
          "name": "gpt-4o-mini via Pokrov"
        }
      }
    }
  }
}
```

3. Select the `gpt-4o-mini` model from provider `pokrov`.
4. Ensure requests use your bearer key (`POKROV_API_KEY`).

## Codex Setup with Pokrov

You can configure a custom provider in `~/.codex/config.toml`:

```toml
model_provider = "pokrov"
model = "gpt-4o-mini"

[model_providers.pokrov]
name = "Pokrov Runtime"
base_url = "http://127.0.0.1:8080/v1"
env_key = "POKROV_API_KEY"
wire_api = "responses"
```

Important:

- Pokrov exposes both `POST /v1/chat/completions` and Codex-compatible `POST /v1/responses`.
- Codex custom providers use the `responses` wire protocol.
- `/v1/responses` is a minimal compatibility layer mapped to the existing sanitization-first proxy pipeline.

### Authentication Modes for Codex

- `auth.gateway_auth_mode: api_key` (default):
  - Gateway auth via `X-Pokrov-Api-Key` or `Authorization: Bearer ...` against `security.api_keys`.
- `auth.gateway_auth_mode: mesh_mtls`:
  - Gateway auth via trusted mesh identity header (default `x-forwarded-client-cert`).
  - Useful for Istio deployments where mTLS is enforced in the service mesh.
- `auth.gateway_auth_mode: internal_mtls`:
  - Gateway auth is bound to transport-verified client certificates (mTLS), not request headers.
  - Runtime config requires `server.tls.*` client-certificate validation settings (`enabled=true`, `require_client_cert=true`, `client_ca_file` set).
  - The configured identity header name (default `x-pokrov-client-cert-subject`) is compatibility metadata only and is not treated as proof of client identity.
  - Example runtime config:

```yaml
server:
  host: 0.0.0.0
  port: 8443
  tls:
    enabled: true
    cert_file: /etc/pokrov/tls/server.crt
    key_file: /etc/pokrov/tls/server.key
    client_ca_file: /etc/pokrov/tls/clients-ca.crt
    require_client_cert: true

auth:
  gateway_auth_mode: internal_mtls
  internal_mtls:
    identity_header: x-pokrov-client-cert-subject
    require_header: true
  upstream_auth_mode: static
```

- `auth.upstream_auth_mode: static`:
  - Use `POKROV_API_KEY` as the gateway credential.
  - Pokrov uses provider credentials from runtime config for upstream calls.
- `auth.upstream_auth_mode: passthrough`:
  - Send gateway credential via `X-Pokrov-Api-Key`.
  - Send provider credential via `Authorization: Bearer ...`.
  - For OpenAI-compatible LLM endpoints (`/v1/chat/completions`, `/v1/responses`),
    single-bearer mode is supported: one `Authorization: Bearer ...` can be used
    for both gateway auth and upstream passthrough credential.

### Routing UX Notes

- `llm.providers[].upstream_path` lets you override the provider upstream endpoint path.
- `llm.routes[].aliases` lets you expose additional model ids that map to the same canonical route.
- `response_envelope.pokrov_metadata.mode` supports:
  - `enabled` (default): include `pokrov` metadata in successful LLM responses
  - `suppressed`: omit `pokrov` metadata for strict-client compatibility

### OpenCode Setup with Pokrov (Split Auth)

OpenCode custom providers support explicit header configuration, so you can keep
gateway and upstream credentials separated in passthrough mode:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "pokrov": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "Pokrov Runtime",
      "options": {
        "baseURL": "http://127.0.0.1:8080/v1",
        "headers": {
          "X-Pokrov-Api-Key": "{env:POKROV_GATEWAY_KEY}",
          "Authorization": "Bearer {env:OPENAI_API_KEY}"
        }
      },
      "models": {
        "gpt-4o-mini": {
          "name": "gpt-4o-mini"
        }
      }
    }
  }
}
```

### Quick Verification for `/v1/responses`

Static mode:

```bash
curl -sS -X POST http://127.0.0.1:8080/v1/responses \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-4o-mini",
    "stream": false,
    "input": "print hello world in rust"
  }' | jq
```

Passthrough mode:

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

- HTTP `200` for valid requests.
- `request_id` in headers/body and `pokrov` metadata in JSON response.
- Metadata-only structured errors on auth/policy/upstream failures.

## Quality Checks

See [docs/configuration.md](docs/configuration.md) for the complete configuration reference with all fields, defaults, and examples.

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --all-targets --all-features
```

Release evidence generation:

```bash
cargo run -p pokrov-runtime -- \
  --release-evidence-output ./config/release/release-evidence.json \
  --release-id hardening-v1 \
  --artifact ./config/pokrov.example.yaml \
  --artifact ./config/release/verification-checklist.md
```
