# Research: LLM Proxy

## Decision 1: Использовать единый OpenAI-compatible endpoint для chat completions

- **Decision**: Зафиксировать `POST /v1/chat/completions` как публичный LLM
  endpoint v1, принимающий OpenAI-compatible request body и возвращающий
  OpenAI-compatible response с дополнительным объектом `pokrov` (metadata-only).
- **Rationale**: Это сохраняет совместимость агентных клиентов и одновременно
  дает explain/audit summary без раскрытия sensitive content.
- **Alternatives considered**:
  - Использовать только `/v1/llm/chat/completions`: добавляет лишний migration
    шаг для OpenAI-совместимых клиентов.
  - Возвращать custom response format: ломает экосистемную совместимость.

## Decision 2: Нормализация запроса выполняется до policy evaluation

- **Decision**: Собирать `LLMRequestEnvelope` из `model`, `messages`, `stream`,
  `metadata.profile` и tool-derived content до sanitization/policy шага.
- **Rationale**: Нормализация гарантирует единый deterministic input для policy
  engine и routing, включая structured handling пустых/невалидных полей.
- **Alternatives considered**:
  - Выполнять sanitization напрямую на raw JSON: усложняет explainability.
  - Делать routing до нормализации: повышает риск расхождений и edge-case drift.

## Decision 3: Profile selection фиксируется как deterministic precedence

- **Decision**: Профиль выбирается в порядке: `metadata.profile` (если валиден)
  -> API key binding profile -> global default profile из config.
- **Rationale**: Такой порядок предсказуем, совместим с dry-run и дает
  контролируемую override-механику без динамического control plane.
- **Alternatives considered**:
  - Только профиль из API key binding: снижает гибкость для интеграций.
  - Только профиль из payload metadata: ослабляет централизованный контроль.

## Decision 4: Routing по модели реализуется через статический map-конфиг

- **Decision**: Использовать валидируемый map `model -> provider_id` с
  fallback policy, где unmapped model возвращает structured error без upstream.
- **Rationale**: Статическая карта маршрутов обеспечивает deterministic
  behavior, readiness-проверяемость и отсутствие runtime guessing.
- **Alternatives considered**:
  - Маршрутизация по regexp/heuristics: усложняет объяснимость и тестируемость.
  - Динамический discovery provider capabilities: out-of-scope для v1.

## Decision 5: Input block path должен short-circuit upstream вызов

- **Decision**: Если input policy action = `block`, endpoint возвращает `403`
  structured policy error с `request_id` и metadata summary, без upstream
  запроса и без sanitized payload в error body.
- **Rationale**: Это строго выполняет sanitization-first и исключает leakage
  через partial passthrough.
- **Alternatives considered**:
  - Возвращать `200` с blocked флагом: нарушает HTTP semantics.
  - Продолжать upstream call с частичным redaction: недопустимый риск.

## Decision 6: Output sanitization включается per-profile и для non-stream, и для stream

- **Decision**: При `output_sanitization=true` ответ upstream проходит policy
  pipeline перед возвратом клиенту; для stream-mode sanitization применяется к
  текстовым `delta`/`content` фрагментам каждого SSE data event.
- **Rationale**: Закрывает FR-008 и позволяет единый security invariant для
  обоих режимов ответа.
- **Alternatives considered**:
  - Делать output sanitization только для non-stream: оставляет leak path.
  - Буферизовать весь stream до конца: ухудшает latency и UX.

## Decision 7: Streaming strategy — SSE pass-through с безопасной event обработкой

- **Decision**: Сохранять OpenAI-style SSE framing (`data: ...\n\n`, `[DONE]`),
  проксировать события по мере поступления, и завершать поток predictably при
  upstream error/timeout структурированным error event.
- **Rationale**: Поддерживается клиентская совместимость и bounded latency,
  при этом сохраняются sanitization guarantees.
- **Alternatives considered**:
  - Преобразовывать SSE в JSON array: ломает совместимость.
  - Raw byte passthrough без event parsing: невозможно безопасно санитизировать.

## Decision 8: Metadata-only audit фиксируется на request lifecycle

- **Decision**: Формировать `LLMAuditEvent` с `request_id`, `profile_id`,
  `provider_id`, `model`, `stream`, `final_action`, `rule_hits_total`,
  `upstream_status`, `duration_ms`, `blocked`.
- **Rationale**: Этого достаточно для operability и explainability без
  включения raw prompt/response или match fragments.
- **Alternatives considered**:
  - Логировать snippets для расследований: противоречит privacy constraints.
  - Логировать только final_action: недостаточно для анализа маршрутизации.

## Decision 9: Upstream failures нормализуются в predictable metadata-only errors

- **Decision**: Timeout/network/provider failures маппятся на `502` или `503`
  с `error.code`, `error.message`, `request_id`, `provider_id` и без payload
  включений; retry hints добавляются только как числовые metadata.
- **Rationale**: Выполняется FR-009/FR-010 и конституционные требования по
  предсказуемому поведению ошибок.
- **Alternatives considered**:
  - Прозрачно прокидывать upstream raw errors: риск leakage.
  - Унифицировать все ошибки в `500`: ухудшает диагностику клиентов.

## Decision 10: Performance neutrality обеспечивается prebuilt clients и bounded buffering

- **Decision**: Инициализировать upstream HTTP clients и routing table на старте,
  избегать per-request reallocation больших буферов, и не добавлять тяжелые
  log/metric операции внутри per-chunk stream loop.
- **Rationale**: Это снижает риск выхода за p95 <= 50 мс для non-stream и
  предотвращает деградацию streaming path.
- **Alternatives considered**:
  - Создавать client per-request: лишний overhead.
  - Детальное логирование каждого SSE chunk: риск latency и leak amplification.
