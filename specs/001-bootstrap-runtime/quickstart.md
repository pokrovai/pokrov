# Quickstart: Bootstrap Runtime

## Preconditions

- Rust stable `1.85+`
- Docker-compatible container runtime для container smoke test
- YAML-конфиг bootstrap runtime
- Секреты передаются через env vars или mounted files, но не в открытом виде в
  YAML

## Example Config

```yaml
server:
  host: 0.0.0.0
  port: 8080

logging:
  level: info
  format: json

shutdown:
  drain_timeout_ms: 5000
  grace_period_ms: 10000

security:
  api_keys:
    - key: env:POKROV_API_KEY
      profile: strict
```

## Local Run

1. Сохранить конфиг как `./config/pokrov.example.yaml`.
   Обязательные поля: `server`, `logging`, `shutdown`.
2. Экспортировать секрет:

```bash
export POKROV_API_KEY='dev-bootstrap-key'
```

3. Запустить runtime:

```bash
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

4. Проверить liveness и readiness:

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
```

5. Убедиться, что ответы содержат `request_id`, а `/ready` возвращает `200`
   только после завершения инициализации.
6. Остановить процесс `Ctrl+C` и убедиться, что shutdown проходит без panic.

## Invalid Config Check

1. Подменить `security.api_keys[0].key` на plain-text значение.
2. Перезапустить сервис.
3. Убедиться, что процесс завершается с понятной config validation error и не
   начинает отвечать `200` на `/ready`.

## Graceful Shutdown Check

1. Запустить сервис и инициировать долгий запрос или открыть keep-alive
   соединение.
2. Послать `SIGTERM`.
3. Проверить, что:
   - runtime переводит `/ready` в not-ready до завершения процесса;
   - lifecycle logs фиксируют `draining` и финальный `stopped`;
   - активные запросы получают время на завершение в пределах
     `drain_timeout_ms`.

## Container Run

1. Собрать контейнерный образ:

```bash
docker build -t pokrov-bootstrap:latest .
```

2. Запустить контейнер с mounted config:

```bash
docker run --rm \
  -p 8080:8080 \
  -e POKROV_API_KEY='dev-bootstrap-key' \
  -v "$(pwd)/config/pokrov.example.yaml:/app/config/pokrov.yaml:ro" \
  pokrov-bootstrap:latest \
  --config /app/config/pokrov.yaml
```

3. Проверить probes:

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
```

4. Выполнить `docker stop` и убедиться, что контейнер проходит через not-ready
   фазу перед завершением.

## Expected Evidence

- Старт с валидным YAML занимает не более 5 секунд на baseline environment.
- Невалидный конфиг не переводит runtime в ready.
- Structured logs содержат lifecycle metadata без raw payload.
- Каждый probe response коррелируется по `request_id` в header/body и логах.
