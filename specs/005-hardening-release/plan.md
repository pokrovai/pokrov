# План реализации: Hardening Release

**Ветка**: `005-hardening-release` | **Дата**: 2026-04-03 | **Спецификация**: [specs/005-hardening-release/spec.md](specs/005-hardening-release/spec.md)
**Вход**: Спецификация фичи из `specs/005-hardening-release/spec.md`

## Сводка

Завершить hardening v1 для self-hosted pilot: добавить deterministic rate limiting
по API key и token-like budget, зафиксировать обязательный набор Prometheus
metrics, доказуемо безопасное structured logging без raw payload leakage,
повторяемые performance/security verification checks и release packaging с
операционной инструкцией и evidence-пакетом.

## Технический контекст

**Language/Version**: Rust stable 1.85+  
**Primary Dependencies**: axum, tokio, tower, tower-http, tracing, tracing-subscriber, serde, serde_yaml, prometheus, uuid  
**Storage**: In-memory rate-limit state + metadata-only audit/log sinks; file-based release evidence artifacts  
**Testing**: `cargo test`, integration tests для throttling/metrics/log-safety, performance checks (p95/p99 + throughput), security checks (abuse, invalid auth, leak assertions), release smoke checks in container  
**Target Platform**: Linux x86_64/aarch64, Docker-compatible self-hosted runtime, baseline CI host  
**Project Type**: Rust web-service workspace  
**Performance Goals**: sanitize+proxy overhead p95 <= 50 мс, p99 <= 100 мс; throughput >= 500 RPS; startup <= 5 сек  
**Constraints**: sanitization-first, metadata-only audit/logging, deterministic policy/rate-limit decisions, no external control plane, no out-of-scope surfaces from PRD  
**Scale/Scope**: v1 hardening/release stage для existing LLM+MCP flows; single-tenant self-hosted pilot readiness

## Проверка конституции

*GATE: Пройдена до Phase 0 research и повторно подтверждена после Phase 1 design.*

### Pre-Design Gate

- **Санитизация до внешнего доступа**: PASS. План сохраняет pre-upstream
  sanitization как обязательный invariant; rate limiting/metrics/logging не
  вводят обходов sanitization path.
- **Детерминированное применение политики**: PASS. Решения по rate limiting,
  token-like estimation и log formatting фиксируются как deterministic при
  одинаковом input/config.
- **Одобренные интерфейсы и ограниченный scope**: PASS. Изменения ограничены
  operational core v1 (rate limiting, metrics, logging safety, verification,
  release package) без A2A/RBAC/SIEM/UI.
- **Наблюдаемость и объяснимые операции**: PASS. В design включены обязательные
  Prometheus series, request_id correlation, predictable 429/upstream errors,
  readiness during degradation и graceful shutdown behavior.
- **Верификация без исключений**: PASS. План включает unit/integration/
  performance/security checks и release evidence как обязательные deliverables.
- **Отклонения от конституции**: PASS. Исключения не требуются.

### Post-Design Re-Check

- `research.md` зафиксировал решения по rate-limit алгоритму, token-like budget
  accounting, telemetry/logging safety и release evidence boundary.
- `data-model.md` описывает сущности hardening-stage без расширения публичного
  product scope за пределы PRD v1.
- `contracts/hardening-api.yaml`, `contracts/metrics-catalog.yaml` и
  `contracts/release-evidence.schema.yaml` формализуют внешние интерфейсы и
  проверочные артефакты без raw payload exposure.
- `quickstart.md` задает repeatable verification path для rate limiting,
  observability safety, performance/security acceptance и release packaging.

## Структура проекта

### Документация фичи

```text
specs/005-hardening-release/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── hardening-api.yaml
│   ├── metrics-catalog.yaml
│   └── release-evidence.schema.yaml
└── tasks.md
```

### Исходный код (корень репозитория)

```text
Cargo.toml
crates/
├── pokrov-core/
│   └── src/
│       ├── detection/
│       ├── transform/
│       ├── policy/
│       └── dry_run/
├── pokrov-proxy-llm/
│   └── src/
│       ├── handlers/
│       ├── routing/
│       └── response/
├── pokrov-proxy-mcp/
│   └── src/
│       ├── mediation/
│       ├── allowlist/
│       └── sanitize/
├── pokrov-api/
│   └── src/
│       ├── middleware/
│       ├── auth/
│       ├── rate_limit/
│       └── routes/
├── pokrov-metrics/
│   └── src/
│       ├── registry.rs
│       ├── exporters.rs
│       └── labels.rs
├── pokrov-config/
│   └── src/
│       ├── loader.rs
│       ├── model.rs
│       └── validate.rs
└── pokrov-runtime/
    └── src/
        ├── lifecycle.rs
        ├── readiness.rs
        └── shutdown.rs
config/
├── pokrov.example.yaml
└── release/
    ├── deployment.env.example
    └── verification-checklist.md
tests/
├── contract/
├── integration/
├── performance/
└── security/
```

**Решение по структуре**: Hardening изменения остаются в текущем Rust workspace:
`pokrov-api` отвечает за auth/rate limit middleware и response semantics,
`pokrov-metrics` за metric contracts/export, `pokrov-runtime` за readiness/shutdown,
`pokrov-config` за limits/telemetry/release config validation, а proxy/core crates
остаются источником sanitization/policy invariants без architectural drift.

## Phase 0: Research Output

- Выбрать deterministic rate-limit strategy (request budget + token-like budget)
  с predictable 429 semantics.
- Зафиксировать безопасный token-like accounting для LLM path без зависимости от
  provider-specific raw telemetry.
- Определить обязательный metrics catalog, label policy и cardinality guardrails.
- Определить logging safety contract: какие поля разрешены и какие данные
  запрещены в structured logs/audit.
- Зафиксировать performance/security verification protocol и структуру release
  evidence package.

## Phase 1: Design Output

- `data-model.md`: hardening entities (rate limit state, metric series,
  logging envelope, release evidence, deployment package manifest).
- `contracts/hardening-api.yaml`: API contract для rate-limited responses и
  observability endpoints.
- `contracts/metrics-catalog.yaml`: mandatory Prometheus series, labels и
  bounded-cardinality rules.
- `contracts/release-evidence.schema.yaml`: schema для release readiness
  artifacts.
- `quickstart.md`: end-to-end verification сценарий для pilot hardening release.

## Complexity Tracking

| Отклонение | Почему необходимо | Почему более простой вариант отклонен |
|------------|-------------------|----------------------------------------|
| Нет отклонений | План укладывается в конституцию и PRD v1 scope | N/A |
