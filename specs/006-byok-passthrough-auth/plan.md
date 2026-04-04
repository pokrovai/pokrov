# План реализации: BYOK Passthrough для шлюза

**Ветка**: `006-byok-passthrough-auth` | **Дата**: 2026-04-04 | **Спецификация**: [specs/006-byok-passthrough-auth/spec.md](specs/006-byok-passthrough-auth/spec.md)
**Вход**: Спецификация фичи из `specs/006-byok-passthrough-auth/spec.md`

## Сводка

Добавить для gateway-сценария режим BYOK passthrough наряду с текущим static
upstream auth, чтобы клиент мог проходить аутентификацию к Pokrov отдельно от
авторизации к целевому LLM/MCP provider. Сохранить sanitization-first поведение,
metadata-only аудит/логи и изоляцию policy/rate-limit по client identity без
выхода за v1 scope.

## Технический контекст

**Language/Version**: Rust stable 1.85+  
**Primary Dependencies**: axum, tokio, tower, serde/serde_json/serde_yaml, tracing, reqwest, uuid, thiserror  
**Storage**: In-memory runtime state для identity bindings/rate-limit counters + metadata-only audit/log sinks  
**Testing**: `cargo test`, contract tests, integration tests, security tests, performance checks  
**Target Platform**: Linux x86_64/aarch64, Docker-compatible self-hosted runtime  
**Project Type**: Rust workspace, multi-crate proxy service (`pokrov-api`, `pokrov-config`, `pokrov-proxy-llm`, `pokrov-proxy-mcp`, `pokrov-runtime`)  
**Performance Goals**: сохранить p95 overhead <= 50 ms, p99 <= 100 ms, throughput >= 500 RPS  
**Constraints**: sanitization-first, deterministic policy decisions, metadata-only audit/logging, no external control plane, secrets only via env/file references  
**Scale/Scope**: v1 gateway hardening для multi-client BYOK без A2A/RBAC/SIEM/UI

## Проверка конституции

*GATE: Пройдена до Phase 0 research и повторно подтверждена после Phase 1 design.*

### Pre-Design Gate

- **Санитизация до внешнего доступа**: PASS. Для обоих auth режимов policy/sanitization проверки остаются pre-upstream и не допускают raw sensitive leakage.
- **Детерминированное применение политики**: PASS. Решения по profile/rate-limit привязаны к client identity и остаются детерминированными для одинаковых входов и конфига.
- **Одобренные интерфейсы и ограниченный scope**: PASS. Изменения ограничены текущими v1 LLM/MCP endpoint и config contracts без новых платформенных модулей.
- **Наблюдаемость и объяснимые операции**: PASS. План включает request_id, structured metadata-only logs, metrics по auth mode/outcome и predictable error mapping.
- **Верификация без исключений**: PASS. Спецификация и план включают unit/integration/security/performance проверки и acceptance evidence.
- **Отклонения от конституции**: PASS. Отклонений не выявлено.

### Post-Design Re-Check

- `research.md` фиксирует решения по dual-mode upstream auth, разделению trust boundaries и безопасной identity binding модели.
- `data-model.md` формализует `UpstreamAuthMode`, `ClientIdentity`, `GatewayAuthContext`, `UpstreamCredentialSource`, `IdentityPolicyBinding` и audit/telemetry сущности.
- Контракты:
  - `contracts/byok-auth-api.yaml` фиксирует HTTP-контракт разделения gateway auth и upstream auth для LLM/MCP endpoint.
  - `contracts/byok-auth-config.yaml` фиксирует контракт конфигурации auth mode и identity binding на уровне runtime config.
- `quickstart.md` описывает проверяемые сценарии `static`/`passthrough`, block paths, metadata-only logging safety и изоляцию лимитов по client identity.

## Структура проекта

### Документация фичи

```text
specs/006-byok-passthrough-auth/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── byok-auth-api.yaml
│   └── byok-auth-config.yaml
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

**Решение по структуре**: Планируемые изменения остаются в существующих границах crate'ов: HTTP/auth/identity extraction в `pokrov-api`, config contracts в `pokrov-config`, upstream auth source resolution в `pokrov-proxy-llm`/`pokrov-proxy-mcp`, readiness/runtime wiring в `pokrov-runtime`, метрики и metadata-only observability в `pokrov-metrics`.

## Phase 0: Research Output

- Выбрать контракт dual-mode upstream auth (`static`/`passthrough`) без разрыва backward compatibility.
- Зафиксировать безопасное разделение gateway auth и upstream provider auth.
- Определить canonical identity source для policy/rate-limit binding.
- Зафиксировать metadata-only logging/audit rules для credential-bearing flows.
- Зафиксировать error semantics для missing/invalid gateway auth и missing/invalid upstream credentials.

## Phase 1: Design Output

- `data-model.md`: сущности auth mode, identity binding, policy/rate-limit resolution и observability envelopes.
- `contracts/`: API и config контракты для BYOK passthrough.
- `quickstart.md`: локальный сценарий верификации режимов и security checks.
- Обновление agent context через `.specify/scripts/bash/update-agent-context.sh codex`.

## Complexity Tracking

| Отклонение | Почему необходимо | Почему более простой вариант отклонен |
|------------|-------------------|----------------------------------------|
| Нет отклонений | План укладывается в PRD и конституционные принципы | N/A |

## Реализация (синхронизация)

- Реализован dual-mode upstream auth (`static`/`passthrough`) для LLM и MCP путей.
- Реализовано разделение gateway auth (`X-Pokrov-Api-Key`/bearer) и upstream credential validation.
- Добавлено identity-bound profile/rate-limit binding через `identity.profile_bindings` и `identity.rate_limit_bindings`.
- Добавлены metadata-only auth-stage audit события и низкокардинальные auth decision metrics.
