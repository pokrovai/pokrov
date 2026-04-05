# Спецификация фичи: Safe Explainability and Audit

**Ветка фичи**: `013-safe-explainability-audit`  
**Дата создания**: 2026-04-05  
**Статус**: Draft  
**Вход**: Описание пользователя: "docs/superpowers/specs/presidio-rework/04-safe-explainability-and-audit.md docs/superpowers/plans/presidio-rework/04-safe-explainability-and-audit-backlog.md"

## Clarifications

### Session 2026-04-05

- Q: What must happen if safe explain/audit generation fails? → A: Mode-based policy: fail-closed for enforcement/runtime paths and fail-open only for explicitly non-enforcing evaluation flows.
- Q: What is the maximum allowed explain/audit overhead per request? → A: <=10ms p95.
- Q: What retention period is required for safe explain/audit metadata? → A: 30 days.
- Q: Who can access safe explain/audit outputs? → A: Only security/ops roles under least-privilege access.

## Пользовательские сценарии и тестирование *(обязательно)*

### Пользовательская история 1 - Verify Safe Explain Output (Приоритет: P1)

A security engineer reviews analyzer decisions and can understand why each decision was made using metadata-only explain output.

**Почему этот приоритет**: Explainability is required to debug policy outcomes safely and to prevent unsafe ad-hoc logging.

**Независимая проверка**: Run analyzer flows and confirm explain output includes only approved metadata fields and never includes raw text fragments.

**Сценарии приемки**:

1. **Given** a request that triggers deterministic recognizers, **When** explain output is produced, **Then** it includes provenance IDs, reason codes, confidence buckets, suppression/overlap markers, and degradation markers only.
2. **Given** a request containing sensitive values, **When** explain output is produced, **Then** raw snippets, nearby context words, and original matched text are absent.

---

### Пользовательская история 2 - Verify Metadata-Only Audit (Приоритет: P2)

An operator investigates runtime behavior and can use audit records to trace decisions without seeing payload content.

**Почему этот приоритет**: Audit is required for operations and incident response, but must preserve metadata-only safety invariants.

**Независимая проверка**: Execute allow/transform/block/degraded flows and confirm audit output always contains only approved metadata fields.

**Сценарии приемки**:

1. **Given** any analyzer execution mode, **When** an audit summary is emitted, **Then** it includes request identity, profile, mode, action, hit counts, path class, execution flags, duration, and degradation metadata only.
2. **Given** payloads with sensitive content, **When** audit records are produced, **Then** no payload values, snippets, or recognizer raw evidence appear.

---

### Пользовательская история 3 - Reuse Safe Outputs Across Reports (Приоритет: P3)

A quality engineer reuses explain and audit outputs in evaluation/parity reporting without requiring a separate unsafe data path.

**Почему этот приоритет**: Reuse prevents duplicated observability models and reduces risk of future schema drift.

**Независимая проверка**: Generate evaluation/parity artifacts from analyzer outputs and confirm they consume the same safe explain/audit fields without adding raw content.

**Сценарии приемки**:

1. **Given** completed analyzer runs, **When** evaluation or parity reports are generated, **Then** reports reuse safe explain and audit fields and preserve metadata-only constraints.

### Edge Cases

- What happens when no recognizers match: explain output still returns a valid metadata-only summary with explicit no-hit reasoning and zero hit counts.
- What happens when remote recognizer degradation occurs: explain and audit output include degradation markers and preserve metadata-only guarantees.
- What happens when reason code mapping is missing for a recognizer outcome: the result uses an explicit fallback reason code and remains serializable.
- What happens when nested payloads are large or deep: output remains metadata-only and does not include any raw fragments from nested structures.
- What happens when safe explain/audit generation fails: enforcement/runtime flows fail closed, while explicitly non-enforcing evaluation flows continue with degradation metadata only.

## Требования *(обязательно)*

### Функциональные требования

- **FR-001**: The system MUST expose a safe explain summary schema limited to approved metadata fields: recognizer provenance IDs, evidence class, reason codes, confidence bucket, validation summary, suppression markers, overlap markers, policy escalation markers, and degradation markers.
- **FR-002**: The system MUST expose an audit summary schema limited to approved metadata fields: request or correlation ID, profile ID, mode, final action, family/category hit counts, path class, execution flags, duration, and degradation metadata.
- **FR-003**: The system MUST enforce structural exclusion of raw payload content from explain and audit outputs, including matched snippets, nearby context words, and recognizer raw evidence.
- **FR-004**: The system MUST provide an explicit, versioned reason-code catalog containing at least: `pattern_match`, `validator_accept`, `validator_reject`, `checksum_valid`, `checksum_invalid`, `context_boost`, `context_suppress`, `allowlist_suppressed`, `denylist_positive`, `overlap_won`, `overlap_lost`, `policy_escalated`, and `remote_degraded`.
- **FR-005**: The system MUST represent explain confidence using stable buckets or score bands suitable for external consumers and must prevent accidental exposure of unsafe raw scoring internals.
- **FR-006**: The system MUST make safe explain and audit outputs reusable by runtime paths and evaluation/parity reporting without introducing additional fields that can carry raw content.
- **FR-007**: The system MUST reject schema regressions through automated tests that fail when prohibited raw-content fields appear in explain or audit outputs.
- **FR-008**: The system MUST enforce mode-based failure handling for explain/audit generation errors: fail closed in enforcement/runtime paths, and permit fail open only in explicitly non-enforcing evaluation flows with degradation metadata.
- **FR-009**: The system MUST restrict read access to safe explain/audit outputs to security/operations roles only, using least-privilege access controls.

### Ключевые сущности *(добавляйте, если фича работает с данными)*

- **Explain Summary**: Metadata-only decision explanation object containing safe provenance, reason, confidence band, validation, suppression/overlap, escalation, and degradation attributes.
- **Audit Summary**: Metadata-only execution record containing request identity, policy profile, decision/action summary, counters, timing, path classification, and degradation attributes.
- **Reason Code Entry**: Versioned catalog item representing a deterministic cause signal used by explain and audit outputs.
- **Confidence Bucket**: Stable confidence band label used for safe external explainability.

## Ограничения безопасности и приватности *(обязательно)*

- Sensitive data includes any payload text, matched fragments, surrounding context strings, secrets, PII values, and recognizer-specific evidence details; these MUST NOT appear in explain or audit outputs.
- Explain and audit models MUST preserve metadata-only safety in allow, transform, block, and degraded paths.
- Any extension of explain/audit fields MUST be explicitly reviewed against metadata-only invariants before release.
- Safe explain/audit metadata retention period MUST be 30 days.
- Access to safe explain/audit outputs MUST be limited to security/operations roles under least-privilege policy.

## Операционная готовность *(обязательно для runtime-изменений)*

- **Логи**: Operational logs must include only request identifiers, decision metadata, reason-code counts, and degradation metadata; logs must not include raw payload-derived values.
- **Метрики**: Observability must expose counts of explain/audit generation success/failure and reason-code/degradation aggregates suitable for trend analysis.
- **Health/Readiness**: Readiness must remain green when safe explain/audit generation is functional; failures in this layer must surface as explicit operational errors.
- **Документация/конфиг**: Operator documentation must include approved explain/audit fields, prohibited data classes, and reason-code catalog governance expectations.
- **Retention**: Safe explain/audit metadata MUST be retained for 30 days and then deleted according to standard operational data lifecycle procedures.

## Required Test Coverage *(обязательно)*

- **Unit**: Validate explain builder and audit builder field-level output contracts and reason-code mapping behavior.
- **Integration**: Validate allow/transform/block/degraded flows produce metadata-only explain and audit outputs, including mode-based failure handling for explain/audit generation errors.
- **Performance**: Validate explain/audit generation overhead stays within `<=10ms p95` per request and does not create measurable regression in the existing analyzer latency budget.
- **Security**: Validate leakage-prevention tests fail on any raw payload, snippet, or context fragment appearing in explain/audit outputs or serialized artifacts, and verify least-privilege read-access restrictions for explain/audit outputs.

## Success Criteria *(обязательно)*

### Измеримые результаты

- **SC-001**: 100% of explain outputs generated for acceptance test flows contain only approved metadata fields and zero prohibited raw-content fields.
- **SC-002**: 100% of audit outputs generated for acceptance test flows contain only approved metadata fields and zero prohibited raw-content fields.
- **SC-003**: 100% of required reason codes are represented in the catalog and covered by automated regression tests.
- **SC-004**: Evaluation/parity reports for acceptance datasets reuse safe explain/audit outputs with zero instances of raw payload leakage.
- **SC-005**: In acceptance performance runs, explain+audit overhead is `<=10ms p95` per request.
- **SC-006**: In retention verification, safe explain/audit metadata remains available for 30 days and is not retained beyond policy limits.
- **SC-007**: In access-control verification, only security/operations roles can retrieve safe explain/audit outputs in all acceptance environments.

## Acceptance Evidence *(обязательно)*

- Provide passing evidence for metadata-only explain output across deterministic recognition scenarios.
- Provide passing evidence for metadata-only audit output across allow, transform, block, and degraded scenarios.
- Provide explicit test evidence that serialized outputs contain no raw sensitive payload fragments.
- Provide report-generation evidence showing safe explain/audit reuse without schema expansion.

## Assumptions

- Existing analyzer contract and deterministic recognition outputs remain available and stable for this feature.
- Existing runtime and evaluation pipelines already consume analyzer outputs and can ingest safe explain/audit schemas once finalized.
- No additional external storage or transport requirements are introduced in this feature scope.
- Reason-code governance will be maintained by explicit catalog updates rather than ad-hoc runtime additions.
