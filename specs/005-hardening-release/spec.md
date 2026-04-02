# Спецификация фичи: Hardening Release

**Ветка фичи**: `005-hardening-release`  
**Дата создания**: 2026-04-03  
**Статус**: Draft  
**Вход**: Описание пользователя: "Завершить hardening v1: rate limiting, metrics, logging safety, performance verification и release packaging для self-hosted pilot."

## Пользовательские сценарии и тестирование *(обязательно)*

### Пользовательская история 1 - Контроль нагрузки и злоупотреблений (Приоритет: P1)

Как оператор self-hosted инсталляции, я хочу ограничивать частоту обращений по API key и объему трафика, чтобы сервис оставался устойчивым при пилотной эксплуатации.

**Почему этот приоритет**: Без rate limiting выпуск v1 не соответствует operational core из PRD.

**Независимая проверка**: Отправить последовательность запросов, превышающих лимит, и убедиться, что сервис возвращает предсказуемый rate-limit response.

**Сценарии приемки**:

1. **Given** клиент превышает configured request budget, **When** сервис продолжает получать запросы, **Then** лишние запросы отклоняются предсказуемо и наблюдаемо.
2. **Given** LLM path позволяет оценить token-like usage, **When** лимит по объему исчерпан, **Then** сервис применяет соответствующее ограничение.

---

### Пользовательская история 2 - Наблюдаемость и безопасность логирования (Приоритет: P2)

Как SRE/AppSec stakeholder, я хочу видеть Prometheus metrics и доказуемо безопасные structured logs, чтобы сопровождать пилот без утечек данных.

**Почему этот приоритет**: Release readiness v1 зависит от observability и logging safety.

**Независимая проверка**: Считать метрики, просмотреть журналы на ключевых сценариях и убедиться, что обязательные метрики присутствуют, а raw payload отсутствует.

**Сценарии приемки**:

1. **Given** рабочая нагрузка на LLM и MCP path, **When** метрики собираются, **Then** оператор видит request, block, error и latency series.
2. **Given** типовые и blocked сценарии, **When** журналы анализируются, **Then** raw prompts, raw arguments и raw outputs отсутствуют.

---

### Пользовательская история 3 - Подтверждение release readiness (Приоритет: P3)

Как команда продукта, я хочу иметь измеримые доказательства готовности self-hosted пилота, чтобы выпуск v1 не зависел от ручных догадок.

**Почему этот приоритет**: Финальная стадия должна подтверждать соответствие NFR и acceptance criteria PRD.

**Независимая проверка**: Выполнить performance, security и operational acceptance checks и собрать релизные артефакты.

**Сценарии приемки**:

1. **Given** полный набор v1 capabilities реализован, **When** выполняются release checks, **Then** результаты подтверждают соответствие latency, reliability и security требованиям PRD.
2. **Given** self-hosted deployment package подготовлен, **When** оператор следует инструкции запуска, **Then** он может развернуть сервис и пройти базовые acceptance checks.

### Edge Cases

- Что происходит, если rate limiting включен, но telemetry backend временно недоступен?
- Как система ведет себя при высокой нагрузке во время graceful shutdown?
- Что происходит, если performance target не выполнен в одном из контрольных сценариев?

## Требования *(обязательно)*

### Функциональные требования

- **FR-001**: Система MUST ограничивать запросы по API key и базовому request budget.
- **FR-002**: Система MUST поддерживать token-like limiting для LLM path там, где возможно безопасно оценить объем запроса.
- **FR-003**: Система MUST публиковать обязательные Prometheus metrics для request count, blocked actions, latency, upstream errors и rate limit events.
- **FR-004**: Система MUST обеспечивать проверяемую безопасность логирования для LLM и MCP путей.
- **FR-005**: Система MUST подтверждать соответствие performance targets v1 через повторяемые проверки.
- **FR-006**: Система MUST подтверждать security acceptance criteria через repeatable checks и documented evidence.
- **FR-007**: Система MUST поставляться в self-hosted release package с инструкцией запуска и базовой верификации.
- **FR-008**: Система MUST документировать release readiness criteria и результаты проверки перед пилотным выпуском.
- **FR-009**: Система MUST возвращать предсказуемые rate-limit ответы без раскрытия sensitive metadata.
- **FR-010**: Система MUST сохранять observability и graceful degradation даже при частичном истощении лимитов или ошибках upstream.

### Ключевые сущности *(добавляйте, если фича работает с данными)*

- **RateLimitPolicy**: Правила ограничения по API key, request budget и token-like usage.
- **MetricSeries**: Набор обязательных telemetry indicators для v1 runtime.
- **ReleaseEvidence**: Пакет результатов performance, security и operational acceptance checks.
- **DeploymentPackage**: Self-hosted артефакт и инструкция запуска пилота.

## Ограничения безопасности и приватности *(обязательно)*

- Rate limiting и observability MUST NOT приводить к журналированию raw payload или новых чувствительных атрибутов.
- Release evidence MUST использовать только безопасные сэмплы, агрегаты и metadata-only результаты.
- Self-hosted package MUST сохранять требование передачи секретов через env vars или secret mounts.

## Операционная готовность *(обязательно для runtime-изменений)*

- **Логи**: MUST появиться записи о rate limiting, upstream errors и release verification без утечек payload.
- **Метрики**: MUST быть доступны все обязательные Prometheus series из PRD.
- **Health/Readiness**: Readiness MUST корректно отражать деградацию, если runtime не способен безопасно обслуживать трафик.
- **Документация/конфиг**: MUST быть подготовлены release notes, deployment guidance и verification checklist.

## Required Test Coverage *(обязательно)*

- **Unit**: Rate limit calculations, telemetry aggregation, safe error formatting.
- **Integration**: Request throttling, metric exposure, logging safety checks, degraded upstream behavior.
- **Performance**: P95/P99 latency overhead, throughput baseline, startup time verification.
- **Security**: Invalid auth, rate-limit abuse, log safety validation, secret handling in deployment.

## Success Criteria *(обязательно)*

### Измеримые результаты

- **SC-001**: P95 latency overhead не превышает 50 мс, а P99 не превышает 100 мс на целевых v1 сценариях.
- **SC-002**: Throughput достигает не менее 500 RPS на baseline payload в контрольном окружении.
- **SC-003**: 100% обязательных Prometheus metrics доступны и подтверждены проверками.
- **SC-004**: 100% релизных лог- и audit-проверок проходят без обнаружения raw sensitive payload.

## Acceptance Evidence *(обязательно)*

- Отчеты performance и security checks.
- Пример rate-limit сценария и соответствующих метрик.
- Self-hosted deployment guide и подтверждение его прохождения.
- Финальный пакет release readiness evidence.

## Assumptions

- Hardening release выполняется после завершения bootstrap, sanitization core, LLM proxy и MCP mediation.
- Pilot release не требует web UI, external control plane или enterprise governance функций.
- Performance verification проводится на baseline payload и baseline инфраструктуре, согласованных в PRD.
