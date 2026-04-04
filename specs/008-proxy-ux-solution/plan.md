# План реализации: Proxy UX P0-P2 Improvements

**Ветка**: `008-proxy-ux-solution` | **Дата**: 2026-04-05 | **Спецификация**: [specs/008-proxy-ux-solution/spec.md](specs/008-proxy-ux-solution/spec.md)
**Вход**: Спецификация фичи из `specs/008-proxy-ux-solution/spec.md`

## Сводка

Расширить Proxy UX roadmap до полного P0+P1+P2: сохранить P0 discovery/aliases/upstream-path/metadata-mode и добавить multi-provider routing с wildcard/prefix и fallback, provider protocol transformers (Anthropic + Gemini), native `/v1/responses` passthrough и provider/model-aware rate limiting. Решение обязано сохранить sanitization-first, deterministic policy behavior, metadata-only audit/logging и bounded v1 архитектуру без выхода в control-plane scope.

## Технический контекст

**Язык/версия**: Rust stable 1.85+  
**Ключевые зависимости**: axum, tokio, tower, serde/serde_json/serde_yaml, tracing, reqwest, uuid, thiserror, bytes, futures-util  
**Хранение данных**: In-memory routing/runtime state + metadata-only audit/log sinks  
**Тестирование**: `cargo test`, contract tests, integration tests, performance checks, security checks  
**Целевая платформа**: Linux x86_64/aarch64, Docker-compatible self-hosted runtime  
**Тип проекта**: Rust workspace, multi-crate proxy service (`pokrov-api`, `pokrov-config`, `pokrov-proxy-llm`, `pokrov-metrics`, `pokrov-runtime`)  
**Цели по производительности**: p95 proxy overhead <= 50 ms, p99 <= 100 ms, throughput >= 500 RPS baseline  
**Ограничения**: sanitization-first, metadata-only audit, deterministic routing/policy, self-hosted only, no external control plane  
**Масштаб/охват**: P0+P1+P2 UX для LLM path: discovery, alias/wildcard/fallback routing, transformers, `/v1/responses` passthrough, provider/model limits

## Проверка конституции

*GATE: Пройдена до Phase 0 research и повторно подтверждена после Phase 1 design.*

### Pre-Design Gate

- **Санитизация до внешнего доступа**: PASS. Все новые сценарии (wildcard/fallback/transform/passthrough) проектируются поверх существующего pre-upstream sanitization порядка.
- **Детерминированное применение политики**: PASS. План фиксирует явный приоритет resolution (exact > alias > wildcard) и предсказуемые fallback trigger rules.
- **Одобренные интерфейсы и ограниченный scope**: PASS. Изменения ограничены v1 self-hosted proxy и существующими endpoint family (`/v1/chat/completions`, `/v1/responses`, `/v1/models`).
- **Наблюдаемость и объяснимые операции**: PASS. План включает request-level logs/metrics для resolution/fallback/transform/rate-limit и readiness fail-fast при невалидном routing graph.
- **Верификация без исключений**: PASS. Для всех новых behavior domains определены unit/integration/performance/security gates и acceptance evidence.
- **Отклонения от конституции**: PASS. Не требуются.

### Post-Design Re-Check

- `research.md` фиксирует решения по wildcard precedence, fallback triggers, transformer strategy, `/v1/responses` passthrough contract и provider/model budgets.
- `data-model.md` описывает routing graph entities, fallback chain, transformer profiles и scoped rate-limit budgets.
- Контракты:
  - `contracts/proxy-ux-api.yaml` покрывает `GET /v1/models`, alias/wildcard routing outcomes, fallback and passthrough semantics.
  - `contracts/proxy-ux-routing-config.yaml` покрывает config contract для aliases/wildcards/fallback/transformers/provider-model limits.
- `quickstart.md` содержит сценарии проверки для discovery, wildcard/fallback routing, Anthropic/Gemini transforms и `/v1/responses` passthrough.

## Структура проекта

### Документация фичи

```text
specs/008-proxy-ux-solution/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── proxy-ux-api.yaml
│   └── proxy-ux-routing-config.yaml
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

**Решение по структуре**: Изменения остаются в существующих crate boundaries. HTTP wiring и response contracts — `pokrov-api`; routing/fallback/transform orchestration — `pokrov-proxy-llm`; config schema/validation — `pokrov-config`; readiness/bootstrap flow — `pokrov-runtime`; observability counters — `pokrov-metrics`; evidence — `tests/*` и `docs/verification/`.

## Phase 0: Research Output

- Зафиксировать deterministic resolution order: exact canonical > alias > wildcard/prefix.
- Зафиксировать fallback activation matrix (какие ошибки считаются retriable для fallback).
- Зафиксировать Anthropic transform boundary: request/response mapping + metadata-only errors.
- Зафиксировать Gemini transform boundary: request/response mapping + metadata-only errors.
- Зафиксировать `/v1/responses` passthrough contract без обязательной деградации в chat-completions subset.
- Зафиксировать provider/model scoped rate-limit модель и совместимость с текущими policy budgets.

## Phase 1: Design Output

- `data-model.md`: сущности routing graph, wildcard rules, fallback chain, transformer profiles, rate-limit scopes.
- `contracts/`: API и config contracts для полного P0+P1+P2 UX scope.
- `quickstart.md`: сценарии локальной проверки discovery, wildcard/fallback, transformers, passthrough и limits.
- Обновление agent context через `.specify/scripts/bash/update-agent-context.sh codex`.

## Complexity Tracking

| Отклонение | Почему необходимо | Почему более простой вариант отклонен |
|------------|-------------------|----------------------------------------|
| Нет отклонений | План укладывается в PRD и конституционные принципы | N/A |
