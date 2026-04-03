# Quickstart: LLM Proxy

## Preconditions

- Rust stable `1.85+`
- Valid Pokrov YAML config with `security.api_keys`, `policies.profiles`, `llm.providers`, `llm.routes`
- Provider API keys configured through `env:` or `file:` secret references
- `jq` for response validation and `curl` with SSE support
- Runtime launched with sanitization core enabled

## Example Config Fragment

```yaml
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict

policies:
  profiles:
    strict:
      output_sanitization: true

llm:
  providers:
    - id: openai
      base_url: https://api.openai.com/v1
      auth:
        api_key: env:OPENAI_API_KEY
      timeout_ms: 30000
      enabled: true
  routes:
    - model: gpt-4o-mini
      provider_id: openai
      output_sanitization: true
```

## Local Run

1. Экспортировать ключи и запустить runtime:

```bash
export POKROV_API_KEY='llm-proxy-test-key'
export OPENAI_API_KEY='provider-test-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

2. Проверить probes:

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
```

## Non-Stream Happy Path Verification

1. Отправить chat completion запрос:

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
    "metadata": {"profile": "strict"}
  }' | jq
```

2. Подтвердить:
- присутствуют `request_id` и `pokrov` metadata;
- `pokrov.action` отражает policy outcome;
- в ответе нет raw sensitive content, если policy потребовала sanitization.

## Block Path Verification

1. Отправить payload, который должен быть заблокирован strict policy.
2. Проверить ответ:
- HTTP `403`;
- structured error с `request_id` и `error.code=policy_blocked`;
- upstream call не выполняется.

## Streaming Verification

1. Отправить stream-запрос:

```bash
curl -N -X POST http://127.0.0.1:8080/v1/chat/completions \
  -H "Authorization: Bearer $POKROV_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{
    "model": "gpt-4o-mini",
    "stream": true,
    "messages": [
      {"role": "user", "content": "Generate a short plan without secrets"}
    ]
  }'
```

2. Подтвердить:
- stream приходит в OpenAI-style SSE формате (`data: ...` + `[DONE]`);
- при включенном output sanitization чувствительные фрагменты не попадают в
  stream chunks;
- `X-Request-Id` возвращается в headers.

## Upstream Error Verification

1. Временно сделать provider недоступным (например, неверный host или key).
2. Повторить запрос и убедиться:
- возвращается `502` или `503`;
- тело ошибки содержит только metadata-only поля;
- нет raw payload или upstream body dump.

## Audit and Logging Safety Verification

1. Выполнить минимум три сценария: allow, block, upstream error.
2. Проверить structured logs:
- присутствуют `request_id`, `provider_id`, `model`, `final_action`, `duration_ms`;
- отсутствуют raw prompt/messages/model outputs.
3. Проверить, что audit summary содержит только counts/actions/routing metadata.

## Performance Verification (Baseline)

1. Выполнить non-stream baseline прогоны (warm-up + 3 измеряемых серии).
2. Зафиксировать p50/p95/p99 proxy overhead.
3. Признать pass, если:
- p95 <= 50 ms;
- p99 <= 100 ms;
- block path deterministic и не вызывает upstream.

## Expected Evidence

- Contract test подтверждает OpenAI-compatible request/response shape.
- Integration tests покрывают happy path, block path, stream path, output sanitization path, upstream error path.
- Security checks подтверждают metadata-only logs/audit и invalid API key handling.
- Performance checks подтверждают latency budget для LLM proxy path.
