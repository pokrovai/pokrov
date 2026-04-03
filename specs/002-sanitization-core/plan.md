# План реализации: Sanitization Core

**Ветка**: `002-sanitization-core` | **Дата**: 2026-04-03 | **Спецификация**: [specs/002-sanitization-core/spec.md](specs/002-sanitization-core/spec.md)
**Вход**: Спецификация фичи из `specs/002-sanitization-core/spec.md`

## Сводка

Реализовать детерминированное sanitization-ядро v1 для evaluate flow: обнаружение
секретов/PII/corporate markers, разрешение пересечений, policy-driven actions
(`allow`, `mask`, `replace`, `redact`, `block`), JSON-safe трансформации,
dry-run и metadata-only audit/explain outputs без утечки raw fragments.

## Технический контекст

**Language/Version**: Rust stable 1.85+  
**Primary Dependencies**: serde, serde_json, serde_yaml, regex, thiserror, axum, tokio, tracing  
**Storage**: In-memory evaluation results + policy profiles from YAML config; metadata-only audit sink (logs/structured events)  
**Testing**: `cargo test`, unit tests (detection/overlap/policy/transform), integration tests (evaluate API + dry-run + block), contract validation, performance/security checks  
**Target Platform**: Linux x86_64/aarch64, Docker-compatible self-hosted runtime  
**Project Type**: Rust web-service workspace + core library crate  
**Performance Goals**: sanitize/policy overhead p95 <= 50 мс, p99 <= 100 мс for baseline payloads  
**Constraints**: sanitization-first before upstream, deterministic outputs for equal input/config, metadata-only audit/logging, no external control plane, strict v1 scope  
**Scale/Scope**: v1 sanitization core as first public evaluate consumer before LLM/MCP forwarding paths

## Проверка конституции

*GATE: Пройдена до Phase 0 research и повторно подтверждена после Phase 1 design.*

### Pre-Design Gate

- **Санитизация до внешнего доступа**: PASS. Evaluate flow проектируется как
  pre-upstream security boundary с запретом raw payload leakage в logs/audit.
- **Детерминированное применение политики**: PASS. План фиксирует
  deterministic ordering для detections и overlap resolution.
- **Одобренные интерфейсы и ограниченный scope**: PASS. В scope только
  sanitization core + evaluate/dry-run + metadata-only audit contracts.
- **Наблюдаемость и объяснимые операции**: PASS. `request_id`, explain summary,
  audit metadata и metrics предусмотрены без добавления out-of-scope surfaces.
- **Верификация без исключений**: PASS. План включает unit/integration/
  performance/security evidence для затронутого behavior.
- **Отклонения от конституции**: PASS. Исключения не требуются.

### Post-Design Re-Check

- `research.md` зафиксировал решения по deterministic detection ordering,
  overlap merge strategy, transform semantics, dry-run parity и audit safety.
- `data-model.md` формализует доменные сущности (`Detection`, `PolicyProfile`,
  `TransformResult`, `AuditSummary`) с явными validation invariants.
- `contracts/sanitization-evaluate-api.yaml` и
  `contracts/policy-profile.schema.yaml` задают публичный evaluate интерфейс и
  policy profile contract без raw content exposure.
- `quickstart.md` определяет repeatable verification сценарий для allow/mask/
  redact/block/dry-run paths и проверок metadata-only outputs.

## Структура проекта

### Документация фичи

```text
specs/002-sanitization-core/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── sanitization-evaluate-api.yaml
│   └── policy-profile.schema.yaml
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
│       ├── traversal/
│       ├── audit/
│       └── dry_run/
├── pokrov-api/
│   └── src/
│       ├── handlers/
│       │   ├── evaluate.rs
│       │   ├── health.rs
│       │   └── ready.rs
│       ├── middleware/
│       └── app.rs
├── pokrov-config/
│   └── src/
│       ├── model.rs
│       ├── loader.rs
│       └── validate.rs
├── pokrov-metrics/
│   └── src/
│       ├── hooks.rs
│       └── registry.rs
└── pokrov-runtime/
    └── src/
        ├── bootstrap.rs
        ├── lifecycle.rs
        └── observability.rs
tests/
├── contract/
├── integration/
├── performance/
└── security/
```

**Решение по структуре**: Добавить `pokrov-core` как изолированный security
engine crate; `pokrov-api` остается HTTP boundary и вызывает core evaluate
pipeline. `pokrov-config` хранит policy profile bindings, а `pokrov-metrics` и
`pokrov-runtime` обеспечивают observability/readiness invariants без
архитектурного дрейфа.

## Phase 0: Research Output

- Зафиксировать category/rule model для built-in и custom detection rules.
- Утвердить deterministic overlap resolution policy при пересечении spans.
- Утвердить transform pipeline order для mixed actions (`mask`, `replace`,
  `redact`, `block`) и block short-circuit.
- Зафиксировать JSON-safe traversal contract: mutate только string leaves,
  сохранить валидность структуры.
- Зафиксировать metadata-only audit/explain schema без raw fragments.
- Зафиксировать dry-run parity: same detections/decision as enforcement без
  unsafe side effects.

## Phase 1: Design Output

- `data-model.md`: сущности detection/policy/transform/audit с validation
  rules и state transitions evaluate flow.
- `contracts/sanitization-evaluate-api.yaml`: HTTP контракт evaluate endpoint с
  deterministic и metadata-only guarantees.
- `contracts/policy-profile.schema.yaml`: schema policy profiles и custom rules
  с ограничениями на actions/priorities.
- `quickstart.md`: локальная проверка evaluate + dry-run + security gates.

## Complexity Tracking

| Отклонение | Почему необходимо | Почему более простой вариант отклонен |
|------------|-------------------|----------------------------------------|
| Нет отклонений | План укладывается в конституцию и PRD v1 scope | N/A |
