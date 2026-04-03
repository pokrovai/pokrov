# Quickstart: Hardening Release

## Preconditions

- Rust stable `1.85+`
- Docker-compatible container runtime
- Valid Pokrov YAML config с включенными секциями rate limiting и metrics
- Test API key через `env:`/`file:` secret reference (без raw ключей в YAML)
- Baseline load-testing tool (например, `k6` или `vegeta`) для performance checks

## Example Config Fragment

```yaml
security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict

rate_limit:
  enabled: true
  default_profile: strict
  profiles:
    strict:
      requests_per_minute: 120
      token_units_per_minute: 24000
      burst_multiplier: 1.5

metrics:
  enabled: true
  bind: 0.0.0.0:9090
```

## Local Run

1. Сохранить конфиг и экспортировать ключ:

```bash
export POKROV_API_KEY='pilot-hardening-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

2. Проверить probes и metrics endpoint:

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
curl -sS http://127.0.0.1:9090/metrics | rg 'pokrov_'
```

3. Убедиться, что ответы содержат `request_id`, а `/metrics` включает mandatory
   series из `contracts/metrics-catalog.yaml`.

## Rate Limit Verification

1. Отправить burst запросов сверх `requests_per_minute`:

```bash
for i in {1..200}; do
  curl -s -o /tmp/resp-$i.json -w '%{http_code}\n' \
    -H "Authorization: Bearer $POKROV_API_KEY" \
    -H 'Content-Type: application/json' \
    -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"test"}]}' \
    http://127.0.0.1:8080/v1/chat/completions
done
```

2. Проверить, что часть ответов имеет `429` и содержит только metadata-only поля:
   `request_id`, `error.code`, `retry_after_ms`, `limit`, `remaining`, `reset_at`.
3. Проверить growth метрик `pokrov_rate_limit_events_total` и
   `pokrov_requests_total{decision="blocked"}`.

## Logging Safety Verification

1. Выполнить типовой allow и blocked сценарий для LLM/MCP path.
2. Проверить structured logs (`stdout` или file sink):
   - присутствуют `request_id`, `decision`, `duration_ms`, `status_code`;
   - отсутствуют raw prompt, raw tool args, raw model/tool outputs;
   - отсутствуют secret-like строки (`sk-`, private key headers, tokens).
3. Зафиксировать результат в security evidence.

## Performance Verification

1. Запустить baseline нагрузочный прогон (warm-up + 3 измеряемых итерации).
2. Собрать метрики p50/p95/p99 latency, throughput и startup time.
3. Признать pass только если одновременно выполняется:
   - p95 <= 50 ms
   - p99 <= 100 ms
   - throughput >= 500 RPS
   - startup <= 5 s

## Security Acceptance Verification

1. Проверить invalid auth сценарии (`401/403` без утечки деталей).
2. Проверить abuse сценарии с превышением budget (`429` + predictable body).
3. Проверить graceful degradation при временных upstream ошибках и во время
   graceful shutdown.
4. Зафиксировать результаты в structured release evidence без payload samples.

## Release Packaging

1. Собрать контейнерный образ:

```bash
docker build -t pokrov-hardening:v1 .
```

2. Сформировать release bundle:
   - image reference/tag;
   - `config/` шаблоны;
   - verification checklist;
   - `release-evidence.json` по schema;
   - checksums для всех артефактов.
3. Прогнать smoke deploy в чистом окружении и подтвердить прохождение
   базовых acceptance checks.

## Expected Evidence

- Все mandatory metrics опубликованы и подтверждены (`100% coverage`).
- Rate limiting воспроизводимо возвращает predictable `429` без leakage.
- Structured logs и audit остаются metadata-only.
- Performance/security gates пройдены в repeatable protocol.
- Release bundle позволяет self-hosted оператору развернуть сервис по инструкции.
