# План реализации: Hardening Release

**Ветка**: `005-hardening-release` | **Дата**: 2026-04-04 | **Спецификация**: [specs/005-hardening-release/spec.md](specs/005-hardening-release/spec.md)
**Вход**: Спецификация фичи из `specs/005-hardening-release/spec.md`

## Сводка

Закрыть hardening scope v1 для self-hosted pilot: deterministic rate limiting
(API key request budget + token-like budget), mandatory Prometheus metrics,
проверяемая logging safety без raw payload leakage, repeatable performance/security
verification и release packaging с metadata-only evidence.

## Технический контекст

**Language/Version**: Rust stable 1.85+  
**Primary Dependencies**: axum, tokio, tower, tower-http, tracing, tracing-subscriber, serde, serde_json, serde_yaml, prometheus, uuid, reqwest, bytes, futures-util, thiserror  
**Storage**: In-memory rate-limit/runtime state + metadata-only audit/log sinks; file-based release evidence artifacts  
**Testing**: `cargo test`, contract tests, integration tests, performance checks, security checks  
**Target Platform**: Linux x86_64/aarch64, Docker-compatible self-hosted runtime  
**Project Type**: Rust workspace, multi-crate proxy service (`pokrov-api` + `pokrov-core` + proxy/runtime crates)  
**Performance Goals**: p95 overhead <= 50 ms, p99 <= 100 ms, throughput >= 500 RPS, startup <= 5 s  
**Constraints**: sanitization-first, deterministic policy decisions, metadata-only audit/logging, no external control plane, secrets only via env/secret mounts  
**Scale/Scope**: v1 pilot hardening scope only (rate limit, metrics, logging safety, release verification, packaging)

## Проверка конституции

*GATE: Пройдена до Phase 0 research и повторно подтверждена после Phase 1 design.*

### Pre-Design Gate

- **Санитизация до внешнего доступа**: PASS. Rate-limit/policy checks и logging safety
  определены как pre-upstream/metadata-only механизмы без raw payload write path.
- **Детерминированное применение политики**: PASS. Для `(api_key_id, profile)`
  фиксируются deterministic budget decisions и predictable error contract (`429`).
- **Одобренные интерфейсы и ограниченный scope**: PASS. Сохранен v1 self-hosted
  scope без A2A, RBAC, SIEM/export pipelines и внешнего control plane.
- **Наблюдаемость и объяснимые операции**: PASS. План включает metrics catalog,
  `request_id`, `/health`, `/ready`, structured metadata-only logging и upstream
  error mapping.
- **Верификация без исключений**: PASS. В spec зафиксированы unit/integration/
  performance/security обязательные проверки и release evidence artifacts.
- **Отклонения от конституции**: PASS. Обязательных отклонений не выявлено.

### Post-Design Re-Check

- `research.md` фиксирует решения по dual-bucket rate limiting, predictable 429
  contract, low-cardinality metrics и metadata-only release evidence.
- `data-model.md` формализует `RateLimitPolicy`, `RateLimitState`,
  `RateLimitDecision`, telemetry/log safety сущности и release evidence model.
- Контракты:
  - `contracts/hardening-api.yaml` определяет API/health/ready/metrics surface,
    predictable rate-limit responses и metadata-only error envelopes.
  - `contracts/metrics-catalog.yaml` фиксирует mandatory Prometheus series и
    запрет high-cardinality/secret-derived labels.
  - `contracts/release-evidence.schema.yaml` задает machine-checkable release
    evidence schema с gate semantics.
- `quickstart.md` описывает repeatable verification: throttling, metrics coverage,
  log safety, performance/security gates и release packaging smoke checks.

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
config/
crates/
├── pokrov-api/
├── pokrov-config/
├── pokrov-core/
├── pokrov-metrics/
├── pokrov-proxy-llm/
├── pokrov-proxy-mcp/
└── pokrov-runtime/
src/
tests/
├── contract/
├── integration/
├── performance/
└── security/
docs/
```

**Решение по структуре**: Изменения hardening распределяются по существующим
crate boundaries: rate limiting и safe HTTP behavior в `pokrov-api`, metrics в
`pokrov-metrics`, proxy-path behavior в `pokrov-proxy-llm`/`pokrov-proxy-mcp`,
shared policy/sanitization invariants в `pokrov-core`, runtime/readiness/release
wiring в `pokrov-runtime`.

## Phase 0: Research Output

- Уточнить алгоритм deterministic dual-bucket limiting и window semantics.
- Зафиксировать безопасный и автоматизируемый контракт `429 rate_limit_exceeded`.
- Зафиксировать metrics catalog с low-cardinality constraints.
- Зафиксировать allowlist-подход для logging safety и metadata-only audit.
- Определить repeatable protocol для performance/security evidence.
- Определить release bundle состав и критерии gate `pass/fail`.

## Phase 1: Design Output

- `data-model.md`: сущности rate limiting, telemetry/log safety и release evidence.
- `contracts/`: API contract, metrics catalog, release evidence schema.
- `quickstart.md`: шаги локальной проверки и release acceptance.
- Обновление agent context через `.specify/scripts/bash/update-agent-context.sh codex`.

## Complexity Tracking

| Отклонение | Почему необходимо | Почему более простой вариант отклонен |
|------------|-------------------|----------------------------------------|
| Нет отклонений | План укладывается в конституцию и PRD v1 scope | N/A |
