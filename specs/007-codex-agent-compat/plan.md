# План реализации: Codex Agent Compatibility

**Ветка**: `007-codex-agent-compat` | **Дата**: 2026-04-04 | **Спецификация**: [specs/007-codex-agent-compat/spec.md](specs/007-codex-agent-compat/spec.md)
**Вход**: Спецификация фичи из `specs/007-codex-agent-compat/spec.md`

## Сводка

Добавить совместимость Pokrov с Codex через новый endpoint `POST /v1/responses`
(минимальный subset для sync и stream), сохранив существующий
`POST /v1/chat/completions` без регрессий. Решение должно сохранить v1 invariants:
sanitization-first до upstream, metadata-only аудит/логи, deterministic policy
behavior, split auth boundary в passthrough (`X-Pokrov-Api-Key` для gateway и
`Authorization: Bearer` для upstream), и текущие наблюдаемость/latency budget.

## Технический контекст

**Language/Version**: Rust stable 1.85+  
**Primary Dependencies**: axum, tokio, tower, serde/serde_json/serde_yaml, tracing, reqwest, uuid, thiserror, bytes, futures-util  
**Storage**: In-memory request/runtime state + metadata-only audit/log sinks  
**Testing**: `cargo test`, contract tests, integration tests, security tests, performance checks  
**Target Platform**: Linux x86_64/aarch64, Docker-compatible self-hosted runtime  
**Project Type**: Rust workspace, multi-crate proxy service (`pokrov-api`, `pokrov-proxy-llm`, `pokrov-config`, `pokrov-metrics`, `pokrov-runtime`)  
**Performance Goals**: сохранить p95 overhead <= 50 ms, p99 <= 100 ms, throughput >= 500 RPS baseline  
**Constraints**: sanitization-first, deterministic policy decisions, metadata-only audit/logging, strict v1 scope, no external control plane  
**Scale/Scope**: v1 LLM compatibility path for Codex (`responses`) без broad Responses parity и без MCP scope expansion

## Проверка конституции

*GATE: Пройдена до Phase 0 research и повторно подтверждена после Phase 1 design.*

### Pre-Design Gate

- **Санитизация до внешнего доступа**: PASS. Новый endpoint проектируется как тонкий compatibility layer поверх существующего pre-upstream sanitization/policy flow.
- **Детерминированное применение политики**: PASS. Mapping `responses -> internal LLM flow` фиксируется детерминированным; policy/action semantics не меняются.
- **Одобренные интерфейсы и ограниченный scope**: PASS. Добавляется только LLM endpoint совместимости; A2A/RBAC/SIEM/UI и broad Responses family остаются out-of-scope.
- **Наблюдаемость и объяснимые операции**: PASS. Для нового route сохраняются request_id, structured logs, auth-stage metrics/events и predictable errors.
- **Верификация без исключений**: PASS. План включает unit/integration/security/performance coverage и acceptance evidence для sync/stream + block paths.
- **Отклонения от конституции**: PASS. Отклонений не требуется.

### Post-Design Re-Check

- `research.md` фиксирует boundary решений по minimal Responses subset, passthrough split-auth и error/stream compatibility.
- `data-model.md` формализует compatibility request/response entities, auth decision context и mapping boundaries.
- Контракты:
  - `contracts/codex-responses-api.yaml` фиксирует API контракт `POST /v1/responses` и совместимость ошибок/stream.
  - `contracts/codex-compat-config.yaml` фиксирует runtime/config assumptions для Codex compatibility path.
- `quickstart.md` задает проверяемые сценарии sync/stream happy paths, auth/policy/upstream block paths и metadata-only safety checks.

## Структура проекта

### Документация фичи

```text
specs/007-codex-agent-compat/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── codex-responses-api.yaml
│   └── codex-compat-config.yaml
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

**Решение по структуре**: Изменения ограничены существующими crate boundaries. HTTP endpoint wiring и auth extraction в `pokrov-api`, request/stream mapping и upstream вызов в `pokrov-proxy-llm`, config validation в `pokrov-config`, observability hooks в `pokrov-metrics`, runtime/docs update в `pokrov-runtime` и `README`.

## Phase 0: Research Output

- Зафиксировать минимальный `responses` subset, достаточный для Codex sync/stream сценариев.
- Зафиксировать deterministic mapping `responses` payload в существующий internal chat-completions flow.
- Зафиксировать split-auth behavior для passthrough без смешения trust boundaries.
- Зафиксировать metadata-only error/observability contract для нового endpoint.
- Зафиксировать non-goal: broad Responses API parity и любые новые v1.1/v2 capabilities.

## Phase 1: Design Output

- `data-model.md`: сущности compatibility request/response, stream events, auth decision summary и mapping constraints.
- `contracts/`: API и config контракты для Codex compatibility path.
- `quickstart.md`: локальная проверка sync/stream, auth block paths, security/observability checks.
- Обновление agent context через `.specify/scripts/bash/update-agent-context.sh codex`.

## Complexity Tracking

| Отклонение | Почему необходимо | Почему более простой вариант отклонен |
|------------|-------------------|----------------------------------------|
| Нет отклонений | План укладывается в PRD и конституционные принципы | N/A |
