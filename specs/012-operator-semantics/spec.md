# Спецификация фичи: Operator Semantics Freeze

**Ветка фичи**: `012-operator-semantics`  
**Дата создания**: 2026-04-05  
**Статус**: Draft  
**Вход**: Описание пользователя: "docs/superpowers/specs/presidio-rework/03-operator-semantics.md docs/superpowers/plans/presidio-rework/03-operator-semantics-backlog.md"

## Clarifications

### Session 2026-04-05

- Q: How should the system handle an unsupported operator reference at runtime? → A: Return fail-closed `block` outcome with metadata-only reason `unsupported_operator`.
- Q: Should `hash` be deterministic? → A: `hash` is always deterministic for identical input within the same profile.
- Q: How should `keep` behave for sensitive matches? → A: `keep` is allowed but MUST be explicitly marked in metadata-only explain/audit outputs.

## Пользовательские сценарии и тестирование *(обязательно)*

### Пользовательская история 1 - Deterministic Operator Outcomes (Приоритет: P1)

As a platform operator, I need the same resolved hit set to always produce the same anonymization outcome so policy enforcement is predictable and defensible.

**Почему этот приоритет**: Deterministic behavior is the foundation for safe enforcement, incident analysis, and trust in policy outcomes.

**Независимая проверка**: Apply identical resolved hits and policy profile multiple times and verify transformed output and decision metadata are identical each run.

**Сценарии приемки**:

1. **Given** identical resolved hits and profile bindings, **When** the transform is executed repeatedly, **Then** output text/JSON and operator decision metadata are identical in every run.
2. **Given** overlapping hits with a deterministic winner after overlap resolution, **When** operators are applied, **Then** only winning spans are transformed and suppressed spans are never re-opened.

---

### Пользовательская история 2 - Clear Block vs Transform Semantics (Приоритет: P2)

As a security owner, I need blocked outcomes to be explicit and isolated from transformed payload delivery so no blocked request can continue downstream.

**Почему этот приоритет**: Incorrect block handling can cause sensitive-data leakage and policy bypass.

**Независимая проверка**: Trigger both blocking and non-blocking policy outcomes and verify blocked flow returns decision metadata without sanitized payload delivery.

**Сценарии приемки**:

1. **Given** final action is `block`, **When** request evaluation completes, **Then** downstream execution does not receive any sanitized payload and safe explain/audit metadata is still produced.
2. **Given** final action is non-blocking, **When** transform executes, **Then** sanitized payload is returned with valid structure and explain/audit metadata.

---

### Пользовательская история 3 - JSON-Safe Structured Processing (Приоритет: P3)

As an integration consumer, I need structured JSON payloads to remain valid after anonymization so upstream and downstream components can process them without schema breakage.

**Почему этот приоритет**: Broken JSON contracts create runtime failures and undermine proxy reliability.

**Независимая проверка**: Run nested JSON objects and arrays through transform and confirm only string leaves are changed while non-string values and structure remain intact.

**Сценарии приемки**:

1. **Given** nested JSON with mixed value types, **When** non-blocking transform is applied, **Then** object/array structure and non-string leaves remain unchanged while string leaves are transformed as required.
2. **Given** a payload with no applicable hits, **When** transform executes, **Then** payload content remains unchanged and still valid JSON.

### Edge Cases

- What happens when multiple operator-eligible hits target adjacent or overlapping spans in one string leaf?
- Unsupported operator references MUST end with fail-closed `block` and metadata-only reason `unsupported_operator`.
- What happens when a `hash` operation is requested for empty strings or strings containing only whitespace?
- `keep` for sensitive content is allowed only as an explicit policy outcome and MUST be marked in metadata-only explain/audit outputs.

## Требования *(обязательно)*

### Функциональные требования

- **FR-001**: The system MUST support exactly five core anonymization operators for transform outcomes: `replace`, `redact`, `mask`, `hash`, and `keep`.
- **FR-002**: The system MUST apply operators only to resolved hits after policy resolution and overlap suppression has completed.
- **FR-003**: The system MUST apply operator actions in a deterministic order so identical resolved-hit inputs produce identical outputs.
- **FR-004**: The system MUST preserve valid JSON for non-blocking outcomes by transforming only string leaves and leaving object/array structure unchanged.
- **FR-005**: The system MUST treat `block` as a terminal decision that prevents any payload forwarding while still returning safe explain and audit metadata.
- **FR-006**: The system MUST allow `keep` only as an explicit intentional policy outcome and MUST mark it in metadata-only safe explain and audit outputs.
- **FR-007**: The system MUST enforce one-way deterministic hash semantics for `hash` so identical input under the same profile yields identical output, and MUST NOT support reversible de-anonymization in core behavior.
- **FR-008**: The system MUST fail closed for any operator reference outside the supported core set by returning final action `block` with metadata-only reason `unsupported_operator`.
- **FR-009**: The system MUST keep transformed-result metadata usable by explain, audit, and evaluation consumers without embedding raw sensitive payloads.

### Ключевые сущности *(добавляйте, если фича работает с данными)*

- **Resolved Hit**: A post-policy matched span with final action context, ordering position, and overlap status used for deterministic transform application.
- **Operator Profile Binding**: Profile-level mapping that selects allowed operator behavior by entity/category context.
- **Transform Result**: Final outcome object representing either blocked decision metadata or sanitized payload plus metadata for explain/audit/evaluation.

## Ограничения безопасности и приватности *(обязательно)*

- Raw sensitive payload content MUST NOT be written into logs, audit records, or explain metadata.
- Blocking decisions MUST prevent payload forwarding and MUST still produce metadata-only operational evidence.
- `hash` behavior MUST remain one-way; reversible anonymization/deanonymization paths are out of scope for this feature.
- Unsupported runtime custom operators MUST NOT execute in core processing.

## Операционная готовность *(обязательно для runtime-изменений)*

- **Логи**: Structured logs include request ID, policy decision, operator counts by type, and blocked/non-blocked routing outcome; no raw payload fragments.
- **Метрики**: Counters and latency metrics distinguish blocked vs transformed outcomes and track operator usage volume.
- **Health/Readiness**: No readiness contract change; feature is considered ready when operator semantics are loaded and deterministic evaluation is available.
- **Документация/конфиг**: Operator semantics reference and profile examples must describe supported operators, blocked semantics, and JSON-validity guarantees.

## Required Test Coverage *(обязательно)*

- **Unit**: Deterministic behavior for each operator, overlap suppression handling, and unsupported-operator rejection.
- **Integration**: Nested JSON flow tests covering non-blocking transform and terminal block behavior.
- **Performance**: Repeated transform runs confirm deterministic results within existing sanitization latency budget.
- **Security**: Tests verify metadata-only explain/audit output and no raw sensitive payload leakage for transformed and blocked paths.

## Success Criteria *(обязательно)*

### Измеримые результаты

- **SC-001**: 100% of repeated runs with identical resolved-hit input produce byte-equivalent transformed output and matching decision metadata.
- **SC-002**: 100% of block decisions prevent downstream payload forwarding while producing explain/audit metadata-only evidence.
- **SC-003**: 100% of non-blocking structured JSON test payloads remain valid JSON with unchanged non-string leaves.
- **SC-004**: 100% of supported operator types are covered by automated tests, including overlap and edge-case scenarios.

## Acceptance Evidence *(обязательно)*

- Unit test report showing deterministic operator outcomes and overlap-aware suppression behavior.
- Integration test report proving separated block path and non-blocking transform path.
- Security evidence showing metadata-only audit/explain outputs with no raw payload leakage.
- Verification artifacts for supported operator set and unsupported-operator safe failure behavior.

## Assumptions

- Resolved hits and policy decisions are already produced by upstream analyzer/policy stages.
- Existing profile configuration model can express operator selection by entity/category without new runtime extension mechanisms.
- Existing observability pipelines can consume additional metadata fields without schema-breaking changes.
- Performance validation reuses the current sanitization latency budget and existing benchmark/test harnesses.
