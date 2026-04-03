# Research: Sanitization Core

## Decision 1: Реализовать detection как набор детерминированных rule packs (built-in + custom)

- **Decision**: Использовать единый `RulePack`, содержащий built-in rules для
  `secrets`, `pii`, `corporate_markers`, плюс custom rules из policy profile.
  Каждое правило имеет стабильные поля `rule_id`, `category`, `priority`,
  `action` и precompiled regex matcher.
- **Rationale**: Единая модель правил упрощает deterministic обработку и
  позволяет одинаково применять safety constraints к built-in и custom rules.
- **Alternatives considered**:
  - Разделять pipelines для built-in и custom rules: сложнее гарантировать
    единые правила overlap resolution.
  - Выполнять custom rules как post-pass: повышает риск semantic drift.

## Decision 2: Overlap resolution фиксировать как stable sort + non-overlapping merge

- **Decision**: После detection сортировать hits по `(start asc, end desc,
  priority desc, rule_id asc)` и выполнять single-pass merge с deterministic
  tie-break logic: при конфликте побеждает hit с более строгим effective action
  (`block > redact > replace > mask > allow`).
- **Rationale**: Такой порядок воспроизводим для одинаковых входов и закрывает
  edge case пересечений нескольких detections на одном фрагменте.
- **Alternatives considered**:
  - Неразрешенные overlap ranges: создает неоднозначные transform results.
  - Random/first-match policy: недетерминированно при изменении порядка rules.

## Decision 3: Трансформации применять только к string leaves JSON-дерева

- **Decision**: Traversal работает по `serde_json::Value` и мутирует только
  строковые листья; объектная/массивная структура сохраняется без изменений.
- **Rationale**: Выполняется требование JSON-safe traversal и FR-005 по
  структурной валидности неблокируемого payload.
- **Alternatives considered**:
  - Трансформировать сериализованную JSON-строку целиком: высокий риск
    повреждения формата.
  - Преобразовывать типы не-строковых полей: нарушает протокольную совместимость.

## Decision 4: `block` трактовать как terminal action без частичного passthrough

- **Decision**: Если policy evaluation выдала `block`, sanitized payload не
  проксируется; ответ содержит только metadata-only decision + explain/audit
  summary. Для неблокирующих исходов возвращается `sanitized_payload`.
- **Rationale**: Соответствует FR-004/FR-005 и исключает частичный unsafe
  passthrough в block path.
- **Alternatives considered**:
  - Частично возвращать трансформированный payload при `block`: ухудшает
    predictability и усложняет клиентские контракты.
  - Подменять `block` на `redact`: меняет security semantics профиля.

## Decision 5: Dry-run сохраняет parity с enforcement по detections и action

- **Decision**: Dry-run выполняет полный detection/policy pipeline и возвращает
  те же `detections`, `final_action`, `explain_summary`, но помечает результат
  `executed=false`; upstream side effects не выполняются.
- **Rationale**: Выполняет FR-008 и поддерживает сравнение behavior без риска
  для production traffic.
- **Alternatives considered**:
  - Упрощенный dry-run без transform/policy этапов: не отражает реальное решение.
  - Dry-run с отдельными rule weights: нарушает deterministic parity.

## Decision 6: Metadata-only audit contract на базе counts и routing context

- **Decision**: Audit event включает `request_id`, `profile_id`, `mode`,
  `rule_hits_total`, `hits_by_category`, `final_action`, `duration_ms`,
  `path_class`. Raw payload, raw fragments и full sanitized text не пишутся.
- **Rationale**: Выполняет FR-010 и constitutional requirement про zero raw
  payload leakage в audit/log paths.
- **Alternatives considered**:
  - Добавлять sample fragments для отладки: высокий риск leakage.
  - Писать только `final_action` без breakdown: недостаточно для explainability.

## Decision 7: Policy profiles задают category actions и custom rule bindings

- **Decision**: Поддерживать profile IDs `minimal`, `strict`, `custom`; для
  каждой категории задавать default action, optional replacement template,
  enablement и override priority. Custom profile может включать user rules, но
  только с разрешенными action enums.
- **Rationale**: Покрывает FR-002/FR-006/FR-007 и сохраняет управляемость
  policy behavior без расширения scope в dynamic policy management.
- **Alternatives considered**:
  - Единый глобальный policy без profiles: не покрывает требуемые режимы.
  - Полностью динамические runtime policy edits: out-of-scope для v1.

## Decision 8: Performance neutrality обеспечить precompiled regex и bounded allocations

- **Decision**: Компилировать regex при загрузке policy config; в hot path
  использовать borrowed slices и pre-sized buffers для transform assembly.
  Дополнительные logs/metrics внутри per-hit loop не добавлять.
- **Rationale**: Снижает latency overhead и соответствует NFR p95 <= 50 мс.
- **Alternatives considered**:
  - Компилировать regex per-request: непредсказуемая деградация latency.
  - Детальный debug logging внутри traversal: повышает overhead и leakage риск.

## Decision 9: Публичный интерфейс фазы — единый evaluate endpoint

- **Decision**: Зафиксировать `POST /v1/sanitize/evaluate` как первый внешний
  интерфейс sanitization core с `mode=enforce|dry_run` и metadata-only summary.
- **Rationale**: Это соответствует assumption, что evaluate flow — первый
  публичный потребитель до интеграции LLM/MCP proxy paths.
- **Alternatives considered**:
  - Только library API без HTTP контракта: ухудшает интеграционную проверяемость.
  - Множественные узкие endpoints per-action: лишняя complexity для v1.
