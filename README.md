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
- BYOK upstream auth mode (`static` and `passthrough`) with strict separation between
  gateway access auth and upstream provider credentials.
- Rate limiting (requests + token units) with `enforce` and `dry_run` modes.
- Health/readiness/metrics endpoints.
- Metadata-only audit and structured logging.
- Release evidence artifact generation.

## Runtime Endpoints

- `GET /health`
- `GET /ready`
- `GET /metrics`
- `POST /v1/sanitize/evaluate`
- `POST /v1/chat/completions`
- `POST /v1/mcp/tool-call`
- `POST /v1/mcp/tools/{toolName}/invoke`

## Out of Scope for v1

Per `docs/PRD.md`, v1 intentionally excludes: A2A proxy, RBAC/IAM, SIEM export, web UI/admin panel, heavy ML NER, response caching, and a full control plane.

## Quick Start (Local)

```bash
export POKROV_API_KEY='dev-runtime-key'
export OPENAI_API_KEY='provider-dev-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
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

- In this repository, Pokrov exposes `POST /v1/chat/completions` and does not expose `POST /v1/responses`.
- Codex custom providers use the `responses` wire protocol.
- For direct Codex integration, you need `responses` compatibility (add `v1/responses` in Pokrov or use an adapter that converts `responses -> chat/completions`).

## Quality Checks

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
