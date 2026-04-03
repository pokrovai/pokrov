# Pokrov Runtime Config

`pokrov.example.yaml` описывает минимальный bootstrap-конфиг для локального и
контейнерного запуска runtime.

## Обязательные секции

- `server.host`, `server.port`
- `logging.level`, `logging.format=json`
- `shutdown.drain_timeout_ms`, `shutdown.grace_period_ms`

Секреты должны передаваться только через `env:` или `file:` references.

## Локальный запуск

```bash
export POKROV_API_KEY='dev-bootstrap-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

Проверка probes:

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
```

## Частая ошибка валидации

`security.api_keys[*].key` в виде plain-text (`my-secret`) будет отклонен.
Разрешены только `env:NAME` и `file:/path`.
