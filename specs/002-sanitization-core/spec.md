# Спецификация фичи: Sanitization Core

**Ветка фичи**: `002-sanitization-core`  
**Дата создания**: 2026-04-03  
**Статус**: Draft  
**Вход**: Описание пользователя: "Реализовать детерминированное ядро sanitization с detection/transformation rules, policy profiles, dry-run и metadata-only audit."

## Пользовательские сценарии и тестирование *(обязательно)*

### Пользовательская история 1 - Оценка payload по политике (Приоритет: P1)

Как security/platform engineer, я хочу прогнать payload через policy engine в dry-run или enforcement режиме, чтобы понять, какие правила сработают и какие действия будут применены.

**Почему этот приоритет**: Это ядро всей ценности Pokrov.AI и фундамент для обоих proxy paths.

**Независимая проверка**: Передать текстовый payload в evaluate flow и убедиться, что движок детерминированно определяет detections, action и explain summary.

**Сценарии приемки**:

1. **Given** payload с секретом и PII, **When** включен strict profile, **Then** система возвращает ожидаемые detections и итоговое action без проксирования upstream.
2. **Given** одинаковый payload и одинаковая конфигурация, **When** evaluate выполняется повторно, **Then** набор detections и action совпадает.

---

### Пользовательская история 2 - Применение трансформаций без поломки формата (Приоритет: P2)

Как инженер интеграции, я хочу получать безопасно трансформированный payload, который остается структурно валидным для дальнейшей обработки.

**Почему этот приоритет**: Если трансформации ломают формат данных, proxy paths становятся непредсказуемыми и непригодными.

**Независимая проверка**: Применить `mask`, `replace`, `redact` и `block` к тестовым payload и убедиться, что результат соответствует политике и сохраняет валидность там, где блокировка не требуется.

**Сценарии приемки**:

1. **Given** payload с несколькими detections, **When** политика требует замены или маскирования, **Then** результат остается пригодным для последующей передачи.
2. **Given** payload, нарушающий правило с action `block`, **When** движок оценивает его, **Then** возвращается block outcome без частичного unsafe passthrough.

---

### Пользовательская история 3 - Metadata-only аудит и explainability (Приоритет: P3)

Как security stakeholder, я хочу видеть метаданные о срабатываниях и итоговом решении без доступа к исходному sensitive content.

**Почему этот приоритет**: Аудит и explainability обязательны для безопасного внедрения и настройки false positives.

**Независимая проверка**: Запустить evaluate/dry-run и проверить, что audit result и explain summary содержат только counts, категории и action, без raw fragments.

**Сценарии приемки**:

1. **Given** evaluate request, **When** rule hits найдены, **Then** аудит содержит counts и profile metadata без raw payload.
2. **Given** dry-run mode, **When** система возвращает explain summary, **Then** summary помогает понять решение без раскрытия чувствительных строк.

### Edge Cases

- Что происходит при пересечении нескольких detections на одном фрагменте?
- Как система ведет себя, если custom rule конфликтует со встроенным правилом?
- Что возвращается, если payload не содержит ни одного срабатывания?

## Требования *(обязательно)*

### Функциональные требования

- **FR-001**: Система MUST обнаруживать как минимум секреты, базовые PII и corporate markers в текстовом payload.
- **FR-002**: Система MUST поддерживать пользовательские правила обнаружения, включаемые через policy profile.
- **FR-003**: Система MUST детерминированно разрешать пересечения detections и возвращать один воспроизводимый набор результатов.
- **FR-004**: Система MUST поддерживать действия `allow`, `mask`, `replace`, `redact` и `block`.
- **FR-005**: Система MUST сохранять структурную валидность неблокируемого payload после трансформации.
- **FR-006**: Система MUST поддерживать policy profiles `minimal`, `strict` и `custom`.
- **FR-007**: Система MUST выбирать и применять policy profile последовательно для каждого evaluate flow.
- **FR-008**: Система MUST предоставлять dry-run режим, который вычисляет решение без фактического proxy execution.
- **FR-009**: Система MUST возвращать explain summary с категориями, числом срабатываний и итоговым action.
- **FR-010**: Система MUST формировать metadata-only audit event без raw payload и без сырых fragments detections.
- **FR-011**: При неоднозначных или конфликтующих частичных трансформациях система MUST возвращать детерминированный итоговый action и детерминированный sanitized result; частичный non-deterministic passthrough запрещен.

### Ключевые сущности *(добавляйте, если фича работает с данными)*

- **Detection**: Факт обнаружения чувствительного контента с категорией, позицией и правилом.
- **TransformResult**: Итог обработки payload с action, sanitized output и служебной сводкой.
- **PolicyProfile**: Набор правил, приоритетов и режимов dry-run/output sanitization.
- **AuditSummary**: Метаданные решения для explainability и безопасного аудита.

## Ограничения безопасности и приватности *(обязательно)*

- Raw payload и сырые detection fragments MUST NOT попадать в аудит, explain summary и логи по умолчанию.
- Dry-run MUST вести себя так же детерминированно, как enforcement, но без unsafe side effects.
- Custom rules MUST подчиняться тем же правилам metadata-only output, что и встроенные правила.

## Операционная готовность *(обязательно для runtime-изменений)*

- **Логи**: MUST фиксироваться событие evaluate и итоговое action без sensitive content.
- **Метрики**: MUST быть подготовлены метрики rule hits, transformed payloads и blocked evaluations.
- **Health/Readiness**: Фича не меняет контракт `/health`, но readiness MUST учитывать успешную загрузку policy configuration.
- **Документация/конфиг**: MUST быть описаны policy profiles, categories и ограничения custom rules.

## Required Test Coverage *(обязательно)*

- **Unit**: Detection rules, overlap resolution, action selection, transform validity, policy priority resolution.
- **Integration**: Evaluate happy path, dry-run, block path, explain output, metadata-only audit path.
- **Performance**: Проверка базового latency overhead для типового evaluate payload.
- **Security**: Проверка отсутствия raw payload в логах, explain outputs и audit artifacts.

## Success Criteria *(обязательно)*

### Измеримые результаты

- **SC-001**: Для одинакового payload и конфигурации 100% повторных evaluate запросов возвращают идентичный набор detections и итоговый action.
- **SC-002**: Не менее 95% типовых evaluate запросов обрабатываются в пределах целевого overhead v1.
- **SC-003**: 100% audit/explain артефактов проходят проверку на отсутствие raw sensitive fragments.
- **SC-004**: Dry-run позволяет воспроизвести итоговое решение политики без фактического proxy execution во всех покрытых сценариях приемки.

## Acceptance Evidence *(обязательно)*

- Набор unit и integration тестов для detection, transform, policy selection и dry-run.
- Демонстрация audit/explain output без sensitive fragments.
- Проверка производительности типового evaluate payload.
- Документация по policy profiles и custom rules.

## Assumptions

- Evaluate flow является первым публичным потребителем sanitization core до подключения LLM и MCP путей.
- На этапе v1 используются regex/custom-rule подходы без heavy ML NER.
- Output sanitization как отдельная capability описывается на уровне policy, но полностью используется следующими proxy-фичами.
- Входные payload для acceptance включают UTF-8 текст и JSON-строки на EN/RU; смешанные языковые payload считаются in-scope для базовых detector категорий.
