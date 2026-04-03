# Data Model: Sanitization Core

## DetectionRule

- **Purpose**: Декларативное правило обнаружения sensitive content.
- **Fields**:
  - `rule_id: String`
  - `category: "secrets" | "pii" | "corporate_markers" | "custom"`
  - `pattern: String`
  - `priority: u16`
  - `action: "allow" | "mask" | "replace" | "redact" | "block"`
  - `replacement_template: Option<String>`
  - `enabled: bool`
- **Relationships**:
  - входит в `PolicyProfile.rules`;
  - используется `DetectionHit.rule_id`.
- **Validation rules**:
  - `rule_id` уникален внутри профиля;
  - `pattern` должен успешно компилироваться в regex на старте;
  - `replacement_template` обязателен только для `action=replace`.

## DetectionHit

- **Purpose**: Конкретное срабатывание правила на строковом фрагменте payload.
- **Fields**:
  - `rule_id: String`
  - `category: String`
  - `start: usize`
  - `end: usize`
  - `action: String`
  - `priority: u16`
- **Validation rules**:
  - `start < end`;
  - диапазон должен попадать в границы исходной строки;
  - list hits перед merge отсортирован deterministic comparator'ом.

## ResolvedSpan

- **Purpose**: Непересекаемый span после overlap resolution.
- **Fields**:
  - `start: usize`
  - `end: usize`
  - `winning_rule_id: String`
  - `effective_action: "allow" | "mask" | "replace" | "redact" | "block"`
  - `suppressed_rule_ids: Vec<String>`
- **Validation rules**:
  - spans не пересекаются между собой;
  - при равном диапазоне winner выбирается deterministic tie-break policy.

## PolicyProfile

- **Purpose**: Набор правил и приоритетов policy evaluation для evaluate flow.
- **Fields**:
  - `profile_id: "minimal" | "strict" | "custom"`
  - `mode_default: "enforce" | "dry_run"`
  - `rules: Vec<DetectionRule>`
  - `category_defaults: CategoryPolicy`
  - `custom_rules_enabled: bool`
- **Relationships**:
  - связывается с `EvaluateRequest.profile_id`;
  - определяет `EvaluateDecision.final_action`.
- **Validation rules**:
  - profile IDs уникальны;
  - `minimal` и `strict` обязательны;
  - custom profile не может задавать action вне enum.

## CategoryPolicy

- **Purpose**: Дефолтные action и параметры трансформации для категорий.
- **Fields**:
  - `secrets_action: String`
  - `pii_action: String`
  - `corporate_markers_action: String`
  - `custom_action: String`
  - `mask_visible_suffix: u8`
- **Validation rules**:
  - action поля принадлежат поддерживаемому enum;
  - `mask_visible_suffix` ограничен диапазоном `0..=8`.

## EvaluateRequest

- **Purpose**: Вход evaluate pipeline.
- **Fields**:
  - `request_id: String`
  - `profile_id: String`
  - `mode: "enforce" | "dry_run"`
  - `payload: serde_json::Value`
  - `path_class: "llm" | "mcp" | "direct"`
- **Validation rules**:
  - `request_id` непустой;
  - `payload` должен быть валидным JSON;
  - `profile_id` должен существовать в загруженной policy config.

## EvaluateDecision

- **Purpose**: Итоговое детерминированное решение policy engine.
- **Fields**:
  - `final_action: "allow" | "mask" | "replace" | "redact" | "block"`
  - `rule_hits_total: u32`
  - `hits_by_category: BTreeMap<String, u32>`
  - `resolved_spans: Vec<ResolvedSpan>`
  - `deterministic_signature: String`
- **Validation rules**:
  - `rule_hits_total == sum(hits_by_category.values())`;
  - `resolved_spans` отсортирован по `start asc`;
  - `deterministic_signature` совпадает при одинаковом input/config.

## TransformResult

- **Purpose**: Результат применения трансформаций к payload.
- **Fields**:
  - `final_action: String`
  - `sanitized_payload: Option<serde_json::Value>`
  - `blocked: bool`
  - `transformed_fields_count: u32`
- **Validation rules**:
  - при `blocked=true` поле `sanitized_payload` равно `None`;
  - при `blocked=false` структура JSON совпадает с входом по форме;
  - transformation меняет только string leaves.

## ExplainSummary

- **Purpose**: Explainability-ответ без раскрытия sensitive fragments.
- **Fields**:
  - `profile_id: String`
  - `mode: String`
  - `final_action: String`
  - `categories: Vec<ExplainCategory>`
  - `rule_hits_total: u32`
- **Validation rules**:
  - элементы `categories` не содержат raw fragments;
  - `rule_hits_total` согласован с `EvaluateDecision`.

## ExplainCategory

- **Purpose**: Категорийная сводка explain output.
- **Fields**:
  - `category: String`
  - `hits: u32`
  - `effective_action: String`
- **Validation rules**:
  - `hits >= 0`;
  - `effective_action` принадлежит action enum.

## AuditSummary

- **Purpose**: Metadata-only аудит evaluate события.
- **Fields**:
  - `request_id: String`
  - `profile_id: String`
  - `mode: String`
  - `final_action: String`
  - `rule_hits_total: u32`
  - `hits_by_category: BTreeMap<String, u32>`
  - `duration_ms: u64`
  - `path_class: String`
- **Validation rules**:
  - raw payload, raw fragments и sanitized text отсутствуют;
  - `duration_ms` заполняется для каждого evaluate запроса;
  - структура пригодна для structured logging и metrics correlation.

## EvaluateResult

- **Purpose**: Композитный результат evaluate API/library вызова.
- **Fields**:
  - `decision: EvaluateDecision`
  - `transform: TransformResult`
  - `explain: ExplainSummary`
  - `audit: AuditSummary`
  - `executed: bool`
- **Validation rules**:
  - при `mode=dry_run` всегда `executed=false`;
  - `decision.final_action == transform.final_action`;
  - все summary-поля согласованы по counts/action.
