# План реализации: LLM Proxy

**Ветка**: `003-llm-proxy` | **Дата**: 2026-04-03 | **Спецификация**: [specs/003-llm-proxy/spec.md](specs/003-llm-proxy/spec.md)
**Вход**: Спецификация фичи из `specs/003-llm-proxy/spec.md`

## Сводка

Реализовать OpenAI-compatible LLM proxy path v1: нормализация chat completions
request, input sanitization до upstream, deterministic provider routing по `model`,
поддержка non-stream/stream ответа, optional output sanitization и metadata-only
audit без утечки raw prompt/response.

## Технический контекст

**Language/Version**: Rust stable 1.85+  
**Primary Dependencies**: axum, tokio, serde, serde_json, tower, tower-http, tracing, uuid, reqwest, bytes, futures-util  
**Storage**: In-memory request context + provider routing/policy bindings from YAML config; metadata-only audit sink (structured logs/events)  
**Testing**: `cargo test`, contract tests для LLM endpoint, integration tests (allow/block/output-sanitize/stream/upstream-failure), performance/security checks  
**Target Platform**: Linux x86_64/aarch64, Docker-compatible self-hosted runtime  
**Project Type**: Rust web-service workspace + dedicated LLM proxy crate  
**Performance Goals**: sanitize+proxy overhead p95 <= 50 мс, p99 <= 100 мс for non-stream baseline; streaming first-byte latency без деградации относительно upstream  
**Constraints**: sanitization-first before upstream, deterministic routing/policy decisions, metadata-only audit/logging, OpenAI-compatible request/response semantics, no external control plane  
**Scale/Scope**: v1 LLM path limited to chat completions with configured providers and profile-bound sanitization

## Проверка конституции

*GATE: Пройдена до Phase 0 research и повторно подтверждена после Phase 1 design.*

### Pre-Design Gate

- **Санитизация до внешнего доступа**: PASS. План фиксирует input sanitization
  перед любым upstream вызовом и block short-circuit без passthrough raw payload.
- **Детерминированное применение политики**: PASS. Выбран stable routing и
  policy evaluation order для идентичных input/config.
- **Одобренные интерфейсы и ограниченный scope**: PASS. Scope ограничен
  OpenAI-compatible chat completions proxy; A2A/RBAC/SIEM/UI отсутствуют.
- **Наблюдаемость и объяснимые операции**: PASS. `request_id`, structured logs,
  metadata-only audit, metrics и predictable upstream errors включены в план.
- **Верификация без исключений**: PASS. Определены unit/integration/
  performance/security checks и acceptance evidence по streaming и block paths.
- **Отклонения от конституции**: PASS. Исключения не требуются.

### Post-Design Re-Check

- `research.md` зафиксировал решения по OpenAI-compatible контракту,
  provider routing, stream handling, output sanitization и audit boundaries.
- `data-model.md` формализует `LLMRequestEnvelope`, `ProviderRoute`,
  `LLMPolicyDecision`, `LLMAuditEvent` и связанные валидации.
- `contracts/llm-proxy-api.yaml` и `contracts/llm-routing.schema.yaml`
  задают интерфейс endpoint и конфигурационный контракт маршрутизации.
- `quickstart.md` определяет repeatable verification для non-stream,
  stream, block, output sanitization и upstream failure сценариев.

## Структура проекта

### Документация фичи

```text
specs/003-llm-proxy/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── llm-proxy-api.yaml
│   └── llm-routing.schema.yaml
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
│       ├── handler.rs
│       ├── normalize.rs
│       ├── routing.rs
│       ├── upstream.rs
│       ├── stream.rs
│       └── audit.rs
├── pokrov-api/
│   └── src/
│       ├── app.rs
│       ├── middleware/
│       └── routes/
├── pokrov-config/
│   └── src/
│       ├── model.rs
│       ├── loader.rs
│       └── validate.rs
├── pokrov-metrics/
│   └── src/
│       ├── registry.rs
│       └── hooks.rs
└── pokrov-runtime/
    └── src/
        ├── bootstrap.rs
        └── lifecycle.rs
tests/
├── contract/
├── integration/
├── performance/
└── security/
```

**Решение по структуре**: Ввести изолированный crate `pokrov-proxy-llm` для
handler/routing/upstream/stream logic, при этом `pokrov-api` остается HTTP
boundary, `pokrov-core` отвечает за sanitization/policy invariants,
`pokrov-config` за provider/profile bindings, а `pokrov-metrics` и
`pokrov-runtime` покрывают observability и lifecycle.

## Phase 0: Research Output

- Зафиксировать OpenAI-compatible chat completions contract и metadata extension
  без дрейфа публичного поведения.
- Утвердить deterministic model-to-provider routing strategy и error semantics
  для unmapped/disabled provider.
- Определить stream handling strategy (SSE pass-through + sanitization safety)
  и правила graceful termination.
- Утвердить output sanitization boundary для non-stream/stream responses.
- Зафиксировать metadata-only audit schema для LLM lifecycle.
- Зафиксировать timeout/retry policy для upstream без нарушения latency budget.

## Phase 1: Design Output

- `data-model.md`: сущности LLM proxy flow с validation rules и state changes.
- `contracts/llm-proxy-api.yaml`: OpenAI-compatible endpoint contract,
  structured errors, stream/non-stream semantics.
- `contracts/llm-routing.schema.yaml`: YAML/schema contract для provider routes
  и profile bindings.
- `quickstart.md`: end-to-end verification path для всех P1/P2/P3 сценариев.

## Complexity Tracking

| Отклонение | Почему необходимо | Почему более простой вариант отклонен |
|------------|-------------------|----------------------------------------|
| Нет отклонений | План укладывается в конституцию и PRD v1 scope | N/A |
