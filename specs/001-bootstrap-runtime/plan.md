# План реализации: Bootstrap Runtime

**Ветка**: `001-bootstrap-runtime` | **Дата**: 2026-04-03 | **Спецификация**: [specs/001-bootstrap-runtime/spec.md](specs/001-bootstrap-runtime/spec.md)
**Вход**: Спецификация фичи из `specs/001-bootstrap-runtime/spec.md`

## Сводка

Сформировать базовый Rust runtime-каркас Pokrov.AI как container-first
self-hosted сервис с валидируемым YAML-конфигом, metadata-only structured logs,
корреляцией по `request_id`, probes `/health` и `/ready`, а также корректным
graceful shutdown. В рамках этой фичи закладывается foundation workspace и
контракты bootstrap-слоя без реализации LLM- и MCP-проксирования.

## Технический контекст

**Language/Version**: Rust stable 1.85+  
**Primary Dependencies**: axum, tokio, serde, serde_yaml, tower, tower-http, tracing, tracing-subscriber, uuid  
**Storage**: N/A  
**Testing**: `cargo test`, интеграционные HTTP lifecycle tests, contract review для OpenAPI и config schema, performance smoke checks для bootstrap endpoints, security checks на отсутствие raw payload/secret leakage  
**Target Platform**: Linux x86_64/aarch64, локальный dev host, Docker/Kubernetes-совместимый container runtime  
**Project Type**: Rust web-service workspace  
**Performance Goals**: startup <= 5 сек; `/health` и `/ready` не должны заметно влиять на runtime budget; архитектура совместима с общими целями p95 <= 50 мс и p99 <= 100 мс overhead  
**Constraints**: self-hosted only, no external control plane, metadata-only observability, secret references only in config, graceful shutdown переводит сервис в not-ready до остановки процесса  
**Scale/Scope**: foundation для v1 runtime; один процесс; baseline readiness/liveness traffic; LLM/MCP business flows, auth enforcement и rate limiting остаются для следующих фич

## Проверка конституции

*GATE: Пройдена до Phase 0 research и повторно подтверждена после Phase 1 design.*

### Pre-Design Gate

- **Санитизация до внешнего доступа**: PASS. Bootstrap-фича не отправляет payload
  upstream; конфиг и логирование проектируются с metadata-only правилами и явным
  запретом raw secret values.
- **Детерминированное применение политики**: PASS. В bootstrap scope
  детерминизм обеспечивается явной lifecycle state machine, строгой схемой
  конфига и воспроизводимым форматом readiness checks.
- **Одобренные интерфейсы и ограниченный scope**: PASS. План ограничен Rust
  self-hosted runtime foundation и не добавляет control plane, UI, A2A или иные
  out-of-scope интерфейсы.
- **Наблюдаемость и объяснимые операции**: PASS. В дизайн включены
  `request_id`, JSON-логи, `/health`, `/ready`, lifecycle events и metrics hooks
  без изменения публичного поведения bootstrap runtime.
- **Верификация без исключений**: PASS. Для bootstrap-поведения определены unit,
  integration, performance и security доказательства.
- **Отклонения от конституции**: PASS. Письменные исключения не требуются.

### Post-Design Re-Check

- `research.md` зафиксировал concrete decisions по workspace layout, схеме
  secret references, lifecycle coordination и structured logging.
- `data-model.md` описывает валидируемый runtime/config/lifecycle domain без
  расширения scope за пределы bootstrap foundation.
- `contracts/runtime-api.yaml` и `contracts/runtime-config.schema.yaml`
  документируют bootstrap HTTP interface и YAML config contract.
- `quickstart.md` задает local/container verification path, достаточный для
  acceptance evidence по startup, readiness и graceful shutdown.

## Структура проекта

### Документация фичи

```text
specs/001-bootstrap-runtime/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── runtime-api.yaml
│   └── runtime-config.schema.yaml
└── tasks.md
```

### Исходный код (корень репозитория)

```text
Cargo.toml
crates/
├── pokrov-api/
│   └── src/
│       ├── app.rs
│       ├── handlers/
│       └── middleware/
├── pokrov-config/
│   └── src/
│       ├── loader.rs
│       ├── model.rs
│       └── validate.rs
├── pokrov-metrics/
│   └── src/
│       ├── hooks.rs
│       └── registry.rs
└── pokrov-runtime/
    └── src/
        ├── bootstrap.rs
        ├── lifecycle.rs
        └── main.rs
config/
├── pokrov.example.yaml
└── README.md
tests/
├── contract/
├── integration/
├── performance/
└── security/
```

**Решение по структуре**: Использовать Rust workspace с отдельными foundation
crates для API, конфигурации, runtime lifecycle и metrics hooks. Такой layout
совпадает с PRD stage 1 и позволяет позже добавить `pokrov-core`,
`pokrov-proxy-llm` и `pokrov-proxy-mcp` без переразметки bootstrap-слоя.

## Phase 0: Research Output

- Зафиксировать формат secret references (`env:` и `file:`) и правила их
  валидации на старте.
- Уточнить способ координации lifecycle: readiness state + active request drain
  без внешнего state store.
- Зафиксировать middleware strategy для `request_id`, structured logs и
  response/header correlation.
- Определить bootstrap contract так, чтобы future readiness checks можно было
  расширять без ломки `/ready`.

## Phase 1: Design Output

- `data-model.md`: RuntimeConfig, SecretRef, RuntimeState, RequestContext,
  probe responses и shutdown policy.
- `contracts/runtime-api.yaml`: JSON-контракт для `/health` и `/ready`.
- `contracts/runtime-config.schema.yaml`: schema для YAML-конфига bootstrap
  runtime.
- `quickstart.md`: локальный и контейнерный сценарий запуска и проверки.

## Complexity Tracking

| Отклонение | Почему необходимо | Почему более простой вариант отклонен |
|------------|-------------------|----------------------------------------|
| Нет отклонений | План укладывается в конституцию и PRD stage 1 | N/A |
