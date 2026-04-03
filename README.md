# Pokrov Runtime

Pokrov is a security-first Rust proxy runtime for AI traffic. The runtime validates
configuration, applies sanitization policy before upstream forwarding, exposes
health/readiness probes, and serves an OpenAI-compatible LLM endpoint.

## Available Endpoints

- `GET /health`
- `GET /ready`
- `POST /v1/sanitize/evaluate`
- `POST /v1/chat/completions`

## Local Start

```bash
export POKROV_API_KEY='dev-runtime-key'
export OPENAI_API_KEY='provider-dev-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

Probe check:

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
```

LLM check:

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

## Container Start

```bash
docker build -t pokrov-runtime:latest .
docker run --rm -p 8080:8080 \
  -e POKROV_API_KEY='dev-runtime-key' \
  -e OPENAI_API_KEY='provider-dev-key' \
  pokrov-runtime:latest
```

## Verification

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --all-targets --all-features
```
