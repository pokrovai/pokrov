# Research: Hardening Release

## Decision 1: Использовать dual-bucket rate limiting по API key (requests + token-like units)

- **Decision**: Для каждого `api_key_id` вести два независимых deterministic
  budget counters: `requests_per_minute` и `token_units_per_minute`; запрос
  допускается только если оба бюджета не исчерпаны.
- **Rationale**: Это закрывает FR-001/FR-002 и предотвращает ситуацию, когда
  малая частота запросов скрывает чрезмерно тяжелые payload.
- **Alternatives considered**:
  - Только request-based лимит: не защищает от больших prompt/tool payload.
  - Только token-like лимит: ухудшает predictability для простых API clients.

## Decision 2: Применять sliding-window counters с monotonic time source

- **Decision**: В hot path использовать lock-efficient in-memory sliding-window
  counters и monotonic clock; ключ состояния: `(api_key_id, window_kind)`.
- **Rationale**: Sliding window дает более предсказуемое распределение нагрузки
  для pilot traffic, чем fixed-window reset spikes, при этом не требует внешнего
  хранилища.
- **Alternatives considered**:
  - Fixed window: проще, но создает burst на границах окна.
  - Внешний Redis-backed limiter: out-of-scope для self-hosted v1 hardening.

## Decision 3: Token-like accounting вычислять до upstream и уточнять post-response

- **Decision**: Для LLM path предварительно оценивать `estimated_units` по
  размеру prompt/messages/tool context до upstream; если provider возвращает
  usage metadata, сохранять его как `observed_units` для metrics/evidence, не
  меняя уже принятого pre-upstream решения.
- **Rationale**: Санитизация-first и deterministic admission требуют решения до
  внешнего вызова; post-response usage полезен для calibration и release checks.
- **Alternatives considered**:
  - Решать лимит только по provider usage после ответа: нарушает pre-upstream
    enforcement.
  - Игнорировать observed usage полностью: снижает quality performance tuning.

## Decision 4: Rate-limit response контракт фиксируется как predictable 429

- **Decision**: При лимите возвращать HTTP `429` с metadata-only JSON body,
  `request_id`, `error.code=rate_limit_exceeded`, `retry_after_ms` и headers
  `Retry-After`, `X-RateLimit-Limit`, `X-RateLimit-Remaining`,
  `X-RateLimit-Reset`.
- **Rationale**: Единая семантика ответа нужна для clients, SRE и automated
  abuse checks; metadata-only формат предотвращает leakage.
- **Alternatives considered**:
  - Возвращать 403/503: семантически неточно для budget exhaustion.
  - Возвращать только текстовую ошибку: недостаточно для автоматизации.

## Decision 5: Metrics catalog ограничивается low-cardinality labels

- **Decision**: Обязательные Prometheus series: `pokrov_requests_total`,
  `pokrov_blocked_total`, `pokrov_upstream_errors_total`,
  `pokrov_rate_limit_events_total`, `pokrov_request_duration_seconds`.
  Labels ограничиваются `route`, `path_class`, `decision`, `provider`, `status`.
- **Rationale**: Закрывает FR-003 и сохраняет bounded memory/cpu footprint без
  high-cardinality risk.
- **Alternatives considered**:
  - Логировать `request_id` как metric label: приводит к cardinality explosion.
  - Отложить metrics catalog до post-release: не соответствует scope hardening.

## Decision 6: Logging safety реализуется allowlist-схемой полей

- **Decision**: Structured logs и audit пишут только allowlisted metadata:
  `timestamp`, `level`, `request_id`, `route`, `policy_profile`, `decision`,
  `rule_hits_count`, `status_code`, `duration_ms`, `rate_limit_bucket`.
  Payload-derived strings, raw tool args, raw model output и secret-like строки
  запрещены.
- **Rationale**: Allowlist-стратегия проще проверять и безопаснее, чем
  blacklist подход в security-critical системе.
- **Alternatives considered**:
  - Blacklist masking в логах: высокий риск пропустить новый sensitive field.
  - Полное отключение логов: теряется operability для pilot support.

## Decision 7: Performance verification фиксируется как repeatable benchmark protocol

- **Decision**: Для release readiness использовать единый baseline protocol:
  warm-up, фиксированный payload set, 3 повторения, запись p50/p95/p99 latency,
  throughput и startup time; acceptance по худшему из трех прогонов.
- **Rationale**: Повторяемая методика исключает ручные интерпретации и
  обеспечивает SC-001/SC-002 traceability.
- **Alternatives considered**:
  - Single-run benchmark: высокий шум, плохая воспроизводимость.
  - Synthetic-only microbench без proxy path: не доказывает end-to-end readiness.

## Decision 8: Release evidence хранить как metadata-only артефактный пакет

- **Decision**: Формировать `release-evidence.json` и прикладывать отчеты
  performance/security/operational checks без raw payload fragments; package
  включает checksums, tool versions, run timestamps и итоговый gate status.
- **Rationale**: Удовлетворяет FR-006/FR-008 и упрощает auditability для pilot
  approval.
- **Alternatives considered**:
  - Хранить только CI logs: недостаточно структурировано для acceptance review.
  - Сохранять sample payload в evidence: нарушает privacy/log-safety constraints.

## Decision 9: Release packaging ориентирован на container-first self-hosted delivery

- **Decision**: Пакет поставки содержит OCI image, `config` примеры,
  deployment/env guidance, verification checklist и команды smoke checks.
- **Rationale**: Соответствует PRD и упрощает воспроизводимый pilot rollout без
  внешнего control plane.
- **Alternatives considered**:
  - Bare binary only: хуже для стандартизированного self-hosted запуска.
  - Helm/operator automation в этой фазе: избыточно для v1 hardening scope.
