# Спецификация фичи: LLM Proxy

**Ветка фичи**: `003-llm-proxy`  
**Дата создания**: 2026-04-03  
**Статус**: Draft  
**Вход**: Описание пользователя: "Добавить OpenAI-compatible LLM proxy с input/output sanitization, provider routing, streaming и metadata-only audit."

## Пользовательские сценарии и тестирование *(обязательно)*

### Пользовательская история 1 - Безопасное проксирование LLM-запроса (Приоритет: P1)

Как AI platform team, я хочу отправлять chat completion запросы через Pokrov.AI, чтобы перед upstream вызовом происходила policy-based sanitization.

**Почему этот приоритет**: Это один из двух главных interaction paths продукта v1.

**Независимая проверка**: Отправить OpenAI-compatible запрос на LLM endpoint и проверить, что разрешенный запрос доходит до upstream только после обработки policy engine.

**Сценарии приемки**:

1. **Given** допустимый запрос с чувствительными фрагментами, **When** strict profile включен, **Then** upstream получает sanitized version, а клиент получает корректный ответ с metadata summary.
2. **Given** запрос, нарушающий block policy, **When** endpoint его обрабатывает, **Then** клиент получает структурированную policy error без upstream вызова.

---

### Пользовательская история 2 - Streaming и routing по модели (Приоритет: P2)

Как интегратор агента, я хочу использовать привычный streaming режим и routing по `model`, чтобы не менять клиентский workflow при подключении Pokrov.AI.

**Почему этот приоритет**: Совместимость с существующими agent workflows снижает стоимость внедрения.

**Независимая проверка**: Отправить non-stream и stream запросы с разными `model` и убедиться, что они маршрутизируются к нужному provider.

**Сценарии приемки**:

1. **Given** запрос со stream=false, **When** выбран provider для модели, **Then** клиент получает стандартный completion response с `request_id`.
2. **Given** запрос со stream=true, **When** upstream поддерживает потоковый режим, **Then** клиент получает SSE-compatible поток без нарушения sanitization guarantees.

---

### Пользовательская история 3 - Output sanitization и auditability (Приоритет: P3)

Как security stakeholder, я хочу по профилю включать output sanitization и видеть metadata-only audit, чтобы ограничивать утечки и при этом понимать итог обработки.

**Почему этот приоритет**: Продуктовая ценность LLM path зависит не только от входной, но и от выходной защиты.

**Независимая проверка**: Включить output sanitization policy и убедиться, что ответ модели при необходимости редактируется до возврата клиенту, а аудит остается metadata-only.

**Сценарии приемки**:

1. **Given** ответ upstream содержит чувствительный фрагмент и профиль требует output sanitization, **When** ответ обрабатывается, **Then** клиент получает sanitized output.
2. **Given** любой LLM request, **When** обработка завершена, **Then** аудит и explain metadata доступны без сырого prompt/response.

### Edge Cases

- Что происходит, если `model` не сопоставлена ни одному configured provider?
- Как система ведет себя при upstream timeout или provider unavailability?
- Что происходит со streaming flow, если block decision принят до начала проксирования?

## Требования *(обязательно)*

### Функциональные требования

- **FR-001**: Система MUST принимать OpenAI-compatible chat completion requests.
- **FR-002**: Система MUST нормализовать входной запрос и извлекать поля, необходимые для policy evaluation и аудита.
- **FR-003**: Система MUST аутентифицировать доступ к LLM proxy endpoint по API key.
- **FR-004**: Система MUST выбирать policy profile для запроса и выполнять input sanitization до upstream proxy.
- **FR-005**: Система MUST блокировать запрос целиком, если итоговое action политики равно `block`.
- **FR-006**: Система MUST маршрутизировать разрешенный запрос к настроенному upstream provider на основе `model`.
- **FR-007**: Система MUST поддерживать базовый streaming mode, совместимый с OpenAI-style SSE.
- **FR-008**: Система MUST применять output sanitization, если это включено выбранным policy profile.
- **FR-009**: Система MUST возвращать metadata summary обработки вместе с ответом, не раскрывая сырой sensitive content.
- **FR-010**: Система MUST формировать metadata-only audit event для каждого LLM flow.
- **FR-011**: Для `model` без активного маршрута (или с disabled route/provider) система MUST возвращать детерминированную структурированную ошибку `invalid_request`/`upstream_unavailable` без fallback к неявному provider.
- **FR-012**: Семантика policy outcome MUST быть одинаковой для stream и non-stream режимов: `block` всегда останавливает upstream до начала проксирования, `allow` всегда сохраняет metadata-only ограничения.

### Ключевые сущности *(добавляйте, если фича работает с данными)*

- **LLMRequestEnvelope**: Нормализованное представление клиентского chat completion запроса.
- **ProviderRoute**: Правило выбора upstream provider по `model`.
- **LLMPolicyDecision**: Итог input/output sanitization и block/allow outcome для LLM flow.
- **LLMAuditEvent**: Метаданные завершенного LLM-запроса.

## Ограничения безопасности и приватности *(обязательно)*

- Сырые prompts, tool-derived context и raw model responses MUST NOT попадать в логи и аудит по умолчанию.
- Output sanitization MUST выполняться до возврата ответа клиенту, если профиль этого требует.
- Ошибки upstream и block responses MUST быть структурированными и не раскрывать sensitive payload.

## Операционная готовность *(обязательно для runtime-изменений)*

- **Логи**: MUST фиксироваться LLM request lifecycle, provider route outcome и policy action без raw content.
- **Метрики**: MUST учитываться request count, blocked requests, latency и upstream errors для LLM path.
- **Health/Readiness**: Readiness MUST учитывать готовность routing configuration для LLM providers.
- **Документация/конфиг**: MUST быть описаны provider mapping, auth expectations и streaming compatibility boundaries.

## Required Test Coverage *(обязательно)*

- **Unit**: Нормализация запроса, profile selection, routing decision, response metadata formatting.
- **Integration**: Happy path, block path, output sanitization path, streaming path, upstream error path.
- **Performance**: Проверка latency overhead на типовом LLM payload и в streaming/non-stream режимах.
- **Security**: Проверка invalid API key, отсутствия raw payload в логах и корректного block behavior.

## Success Criteria *(обязательно)*

### Измеримые результаты

- **SC-001**: 100% LLM запросов проходят input sanitization до upstream proxy.
- **SC-002**: Не менее 95% типовых non-stream LLM запросов укладываются в NFR latency overhead v1.
- **SC-003**: 100% policy block сценариев завершаются без upstream call.
- **SC-004**: 100% проверенных audit/log артефактов по LLM path не содержат raw prompts или raw responses.
- **SC-005**: Не менее 99% типовых запросов на baseline-нагрузке завершаются без transport/internal ошибки при доступном upstream provider.

## Acceptance Evidence *(обязательно)*

- Интеграционные тесты happy path, block path, streaming и output sanitization.
- Подтверждение provider routing для нескольких `model`.
- Проверка структурированных ошибок invalid auth и upstream failure.
- Аудит и лог-сэмплы без sensitive content.

## Assumptions

- В scope входит только OpenAI-compatible chat completion surface, без дополнительных LLM API family.
- Provider routing опирается на заранее заданную конфигурацию, а не на динамический control plane.
- Input/output sanitization использует детерминированный sanitization core из предыдущей фичи.
- OpenAI-compatible scope для v1 ограничен `/v1/chat/completions` (JSON и SSE), без function-calling orchestration surface, embeddings и assistants API.
- Baseline reliability/latency acceptance проводится при фиксированных таймаутах upstream и неизменном наборе типовых payload из verification runbook.
