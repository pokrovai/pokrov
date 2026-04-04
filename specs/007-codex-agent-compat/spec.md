# Спецификация фичи: Codex Agent Compatibility

**Ветка фичи**: `007-codex-agent-compat`  
**Дата создания**: 2026-04-04  
**Статус**: Draft  
**Вход**: Описание пользователя: "реализовать совместимость Pokrov с coding agent Codex"

## Пользовательские сценарии и тестирование *(обязательно)*

### Пользовательская история 1 - Codex request compatibility (Приоритет: P1)

As a platform team integrating Codex with Pokrov, I can send Codex-compatible LLM requests to Pokrov and receive successful responses without bypassing security controls.

**Почему этот приоритет**: This is the primary adoption path. Without request compatibility, Codex cannot use Pokrov as a secure gateway.

**Независимая проверка**: Send a valid Codex-style request to the new LLM compatibility endpoint and verify successful completion through policy and sanitization flow.

**Сценарии приемки**:

1. **Given** Pokrov is configured with a valid model route and passthrough mode, **When** a Codex-compatible non-stream request is sent to `POST /v1/responses`, **Then** Pokrov returns a successful structured response.
2. **Given** a request containing sensitive data markers, **When** the request passes through the new endpoint, **Then** sanitization and policy enforcement are applied before any upstream call.

---

### Пользовательская история 2 - Streaming parity for Codex workflows (Приоритет: P2)

As a Codex operator, I can use streaming mode through Pokrov and receive safe streamed output with deterministic policy behavior.

**Почему этот приоритет**: Streaming is required for normal coding-agent interaction quality and responsiveness.

**Независимая проверка**: Execute a stream request through `POST /v1/responses` and verify SSE lifecycle, sanitization guarantees, and stable completion semantics.

**Сценарии приемки**:

1. **Given** an upstream stream-capable model route, **When** a Codex-compatible stream request is sent, **Then** Pokrov returns an SSE-compatible stream with safe output.
2. **Given** policy detects output requiring transform or block, **When** stream chunks are processed, **Then** Pokrov enforces profile actions without exposing raw sensitive data.

---

### Пользовательская история 3 - Secure dual-auth passthrough boundary (Приоритет: P3)

As a security stakeholder, I can enforce strict separation between gateway authentication and upstream provider credentials for Codex traffic.

**Почему этот приоритет**: Preserving split-auth is required to keep the v1 security boundary and prevent credential-scope confusion.

**Независимая проверка**: Validate that gateway auth and upstream credential checks are independent and produce deterministic metadata-only errors.

**Сценарии приемки**:

1. **Given** valid gateway auth in `X-Pokrov-Api-Key` and valid upstream bearer token in `Authorization`, **When** a request is submitted, **Then** Pokrov allows upstream processing.
2. **Given** valid gateway auth but missing upstream bearer token in passthrough mode, **When** the same request is submitted, **Then** Pokrov returns a structured auth error before upstream call.

### Edge Cases

- What happens when a Codex-compatible request contains unsupported optional fields outside the minimal v1 subset?
- How does the system behave when stream output contains malformed JSON event chunks from upstream?
- What is returned when gateway auth passes but upstream provider returns `401` or `403`?
- What happens if an endpoint consumer attempts to use one bearer credential for both gateway and upstream identities?

## Требования *(обязательно)*

### Функциональные требования

- **FR-001**: The system MUST expose `POST /v1/responses` for Codex compatibility while preserving existing `POST /v1/chat/completions` behavior.
- **FR-002**: The system MUST support a minimal Codex-compatible request/response subset for both non-stream and stream modes on `POST /v1/responses`.
- **FR-003**: The system MUST preserve sanitization-first behavior on the new endpoint, applying policy evaluation and transformations before upstream forwarding.
- **FR-004**: The system MUST preserve metadata-only audit semantics for all `POST /v1/responses` outcomes.
- **FR-005**: The system MUST enforce dual-auth separation in passthrough mode: gateway access credential and upstream provider credential are validated independently.
- **FR-006**: In passthrough mode, the system MUST require gateway credential via `X-Pokrov-Api-Key` and MUST treat upstream `Authorization: Bearer` as upstream-only credential for Codex compatibility path.
- **FR-007**: The system MUST return deterministic structured errors for invalid gateway auth, missing upstream credential, policy block, and upstream failure on the new endpoint.
- **FR-008**: The system MUST apply identity-bound profile and rate-limit selection for the new endpoint using the existing resolution order.
- **FR-009**: The system MUST emit observability signals (request outcome, latency, auth decisions, upstream error classes) for `POST /v1/responses` consistent with existing LLM endpoint semantics.
- **FR-010**: The system MUST document supported Codex compatibility scope and explicitly exclude broad Responses API parity in v1.
- **FR-011**: The system MUST NOT introduce new non-v1 capabilities (A2A, RBAC, SIEM export, web UI, control-plane features) as part of this feature.

### Ключевые сущности *(добавляйте, если фича работает с данными)*

- **Codex Response Request**: Minimal Codex-compatible request shape accepted by `POST /v1/responses` and mapped into Pokrov LLM processing.
- **Codex Response Stream Event**: Stream event unit returned in SSE mode after policy and sanitization handling.
- **Gateway Credential**: Access credential used only for Pokrov gateway authorization.
- **Upstream Credential**: Provider bearer credential used only for upstream model authorization in passthrough mode.
- **Compatibility Decision Context**: Metadata-only decision set including profile, action, rule-hit counts, auth-stage outcomes, and endpoint routing context.

## Ограничения безопасности и приватности *(обязательно)*

- Raw request payloads, raw model outputs, gateway credentials, and upstream credentials MUST NOT appear in logs, metrics, or audit events.
- Policy `allow/mask/redact/block` enforcement MUST remain deterministic and unchanged in meaning for the new endpoint.
- Gateway and upstream auth scopes MUST remain separated; a valid upstream token alone MUST NOT grant Pokrov gateway access.
- Structured error payloads MUST remain metadata-only and MUST NOT leak secret fragments.

## Операционная готовность *(обязательно для runtime-изменений)*

- **Логи**: Structured logs MUST include request id, route, decision outcome, and auth stage results without sensitive payloads.
- **Метрики**: Metrics MUST include request counts/outcomes, auth-stage decisions, upstream errors, and latency for the new endpoint.
- **Health/Readiness**: Existing `/health` and `/ready` contracts MUST remain backward compatible.
- **Документация/конфиг**: Runtime documentation MUST define Codex compatibility constraints, passthrough dual-auth usage, and migration guidance from `chat/completions` to `responses` endpoint consumers.

## Required Test Coverage *(обязательно)*

- **Unit**: Request normalization/mapping, split-auth validation, structured error mapping, and endpoint-level policy context composition.
- **Integration**: Non-stream happy path, stream happy path, missing-upstream-credential block path, gateway-auth failure path, and upstream `4xx/5xx` handling.
- **Performance**: Verify additional endpoint processing preserves v1 overhead target and does not exceed existing latency budget envelope.
- **Security**: Verify metadata-only logging/audit safety and absence of raw secrets across success, block, and upstream-failure paths.

## Success Criteria *(обязательно)*

### Измеримые результаты

- **SC-001**: 100% of Codex compatibility acceptance tests for non-stream `POST /v1/responses` pass in CI.
- **SC-002**: 100% of Codex compatibility acceptance tests for stream `POST /v1/responses` pass in CI.
- **SC-003**: 100% of security verification checks confirm no raw sensitive payload or credential leakage in logs, metrics, or audit events for the new endpoint.
- **SC-004**: Existing `POST /v1/chat/completions` contract tests remain green with no behavioral regressions.
- **SC-005**: Measured P95 proxy overhead remains within the v1 target budget under baseline compatibility test load.

## Acceptance Evidence *(обязательно)*

- Contract and integration test results proving non-stream and stream compatibility behavior for Codex scenarios.
- Evidence bundle for auth-failure, policy-block, and upstream-failure paths showing structured metadata-only outcomes.
- Logging and audit validation artifacts proving zero raw payload and credential leakage.
- Updated runtime documentation and usage examples for Codex integration with dual-auth passthrough.

## Assumptions

- Codex integration consumers can send gateway and upstream credentials in separate headers when passthrough mode is enabled.
- v1 compatibility scope is limited to LLM path and does not include full broad Responses API parity.
- Existing sanitization engine, policy semantics, and audit model remain authoritative and unchanged.
- Existing chat-completions behavior stays fully supported during compatibility rollout.
