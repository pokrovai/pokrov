# Pokrov Bootstrap Runtime

Bootstrap runtime foundation для Pokrov.AI: валидируемый YAML-конфиг,
`/health` и `/ready` probes, `request_id` correlation, structured JSON logs и
graceful shutdown lifecycle.

## Local Start

```bash
export POKROV_API_KEY='dev-bootstrap-key'
cargo run -p pokrov-runtime -- --config ./config/pokrov.example.yaml
```

Проверка:

```bash
curl -sS http://127.0.0.1:8080/health
curl -sS -i http://127.0.0.1:8080/ready
```

Остановка: `Ctrl+C`. Runtime переводится в `draining` (`/ready` => `503`) до
завершения in-flight запросов или истечения `drain_timeout_ms`.

## Container Start

```bash
docker build -t pokrov-bootstrap:latest .
docker run --rm -p 8080:8080 -e POKROV_API_KEY='dev-bootstrap-key' pokrov-bootstrap:latest
```

## Verification

```bash
cargo test --workspace
cargo fmt --check
cargo clippy --all-targets --all-features
```

