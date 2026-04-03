# План реализации: MCP Mediation

**Ветка**: `004-mcp-mediation` | **Дата**: 2026-04-04 | **Спецификация**: [specs/004-mcp-mediation/spec.md](specs/004-mcp-mediation/spec.md)
**Вход**: Спецификация фичи из `specs/004-mcp-mediation/spec.md`

## Сводка

Реализовать MCP mediation path v1 через `POST /v1/mcp/tool-call`: до upstream
выполнения enforce server/tool allowlist + argument validation, на блокирующем
пути возвращать структурированную metadata-only ошибку, на allow-пути
санитизировать tool output и фиксировать metadata-only audit без утечки raw
arguments/output.

## Технический контекст

**Language/Version**: Rust stable 1.85+  
**Primary Dependencies**: axum, tokio, serde, serde_json, serde_yaml, tower, tracing, uuid, reqwest, thiserror  
**Storage**: In-memory request context + statically loaded MCP policy/config from YAML; metadata-only audit/log sink  
**Testing**: `cargo test`, contract tests для MCP endpoint/config schema, integration tests (allow/block/arg-invalid/output-sanitize/upstream-failure), performance/security checks  
**Target Platform**: Linux x86_64/aarch64, Docker-compatible self-hosted runtime  
**Project Type**: Rust web-service workspace + dedicated `pokrov-proxy-mcp` crate  
**Performance Goals**: MCP mediation overhead p95 <= 50 мс, p99 <= 100 мс на типовом pilot tool call; block path short-circuit без upstream latency  
**Constraints**: sanitization-first для tool output, deterministic allowlist/policy decisions, metadata-only audit/logging, fixed pilot subset без полного MCP transport coverage, no external control plane  
**Scale/Scope**: v1 MCP subset: один HTTP mediation surface, статический allowlist server/tool, schema+policy argument checks, predictable upstream error mapping

## Проверка конституции

*GATE: Пройдена до Phase 0 research и повторно подтверждена после Phase 1 design.*

### Pre-Design Gate

- **Санитизация до внешнего доступа**: PASS. Block path выполняется до upstream,
  output sanitization применяется до возврата результата агенту.
- **Детерминированное применение политики**: PASS. Фиксируется явный порядок
  `server allowlist -> tool allowlist/blocklist -> argument validation -> action`.
- **Одобренные интерфейсы и ограниченный scope**: PASS. Только MCP mediation
  subset v1, без registry/control-plane/RBAC/A2A.
- **Наблюдаемость и объяснимые операции**: PASS. `request_id`, metadata-only
  audit/log, metrics и predictable upstream errors включены в артефакты.
- **Верификация без исключений**: PASS. Определены unit/integration/
  performance/security checks и acceptance evidence.
- **Отклонения от конституции**: PASS. Исключения не требуются.

### Post-Design Re-Check

- `research.md` зафиксировал решения по pilot subset endpoint, детерминированной
  policy precedence, validation contract и safe error semantics.
- `data-model.md` формализует `McpToolCallRequest`, `McpToolPolicyDecision`,
  `ToolValidationResult`, `McpAuditEvent` и related invariants без payload leakage.
- `contracts/mcp-mediation-api.yaml` и `contracts/mcp-policy.schema.yaml`
  задают публичный endpoint контракт и конфигурационный контракт allowlist/policy.
- `quickstart.md` описывает repeatable verification для allowed, blocked,
  invalid-arguments, sanitized-output и upstream-unavailable сценариев.

## Структура проекта

### Документация фичи

```text
specs/004-mcp-mediation/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── mcp-mediation-api.yaml
│   └── mcp-policy.schema.yaml
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
├── pokrov-proxy-mcp/
│   └── src/
│       ├── handler.rs
│       ├── policy.rs
│       ├── validate.rs
│       ├── upstream.rs
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

**Решение по структуре**: Ввести изолированный `pokrov-proxy-mcp` для mediation,
policy/validation/upstream/output-sanitize logic. `pokrov-api` остается HTTP
boundary и auth/rate-limit точкой входа, `pokrov-core` сохраняет sanitization
invariants, `pokrov-config` отвечает за статическую валидируемую MCP-конфигурацию,
`pokrov-metrics`/`pokrov-runtime` покрывают operability.

## Phase 0: Research Output

- Зафиксировать pilot-ready MCP mediation subset и explicit out-of-scope по
  transport variants.
- Утвердить deterministic precedence для allowlist/blocklist/argument validation.
- Определить безопасный формат block/explain response без raw arguments/output.
- Определить output sanitization boundary для structured tool result JSON.
- Зафиксировать upstream timeout/unavailable/error mapping на predictable
  metadata-only клиентские ошибки.
- Зафиксировать readiness/metrics требования MCP path для acceptance evidence.

## Phase 1: Design Output

- `data-model.md`: сущности MCP mediation flow, validation rules и state
  transitions для allow/block/outcome.
- `contracts/mcp-mediation-api.yaml`: HTTP contract для endpoint
  `/v1/mcp/tool-call`, block errors и safe success response.
- `contracts/mcp-policy.schema.yaml`: schema/validation contract для
  allowlisted servers/tools и argument constraints.
- `quickstart.md`: пошаговая verification инструкция для unit/integration/
  performance/security evidence MCP path.

## Complexity Tracking

| Отклонение | Почему необходимо | Почему более простой вариант отклонен |
|------------|-------------------|----------------------------------------|
| Нет отклонений | План укладывается в конституцию и PRD v1 scope | N/A |
