# Research: MCP Mediation

## Decision 1: Зафиксировать единый MCP mediation endpoint для v1

- **Decision**: Использовать `POST /v1/mcp/tool-call` как единственный публичный
  MCP endpoint v1 для pilot subset.
- **Rationale**: Контракт из PRD уже описывает этот surface; это минимизирует
  интеграционный риск и сохраняет bounded scope.
- **Alternatives considered**:
  - Вводить несколько endpoint-ов (`/invoke`, `/resources`, `/prompts`): расширяет
    scope и усложняет verification.
  - Реализовывать полный нативный MCP transport coverage в v1: out-of-scope.

## Decision 2: Явно ограничить transport scope предсказуемым отказом

- **Decision**: Поддерживать только JSON HTTP facade subset для tool-call; любые
  неподдерживаемые transport variants отклонять deterministic structured error.
- **Rationale**: Требование FR-008 требует pilot-ready subset без полного MCP
  coverage.
- **Alternatives considered**:
  - Молчаливый fallback на нестандартизованные transport semantics: ломает
    предсказуемость.
  - Попытка частичной auto-detection transport type: повышает риск drift.

## Decision 3: Применять policy в фиксированном порядке с blocklist precedence

- **Decision**: Policy order: `server allowlist` -> `tool allowlist` ->
  `tool blocklist precedence` -> `argument validation` -> `upstream call`.
- **Rationale**: Детерминированный порядок предотвращает обход block path и
  дает воспроизводимые результаты при одинаковом config.
- **Alternatives considered**:
  - Проверять аргументы до allowlist: лишняя нагрузка и утрата explainability.
  - Разрешать tool при наличии в allowlist, даже если он в blocklist:
    противоречит security intent.

## Decision 4: Аргументы валидировать в две стадии

- **Decision**: Выполнять сначала schema validation (если schema задана), затем
  policy constraints validation; при отсутствии schema применять policy constraints.
- **Rationale**: Покрывает edge-case из spec (schema может отсутствовать), но
  сохраняет контроль и block-before-execution.
- **Alternatives considered**:
  - Требовать schema для каждого tool: снижает practical adoption в пилоте.
  - Только schema без policy constraints: недостаточный контроль рисковых аргументов.

## Decision 5: Block path возвращает безопасный explain summary

- **Decision**: Для policy-нарушений возвращать `403` с
  `error.code=tool_call_blocked`, `allowed=false`, metadata-only `details`
  (`server`, `tool`, `reason`) без raw arguments.
- **Rationale**: Выполняет FR-006/FR-010 и сохраняет приватность.
- **Alternatives considered**:
  - Возвращать сырые аргументы в `details`: leakage risk.
  - Возвращать только generic message без metadata: ухудшает operability.

## Decision 6: Upstream failures маппить на predictable metadata-only ошибки

- **Decision**: Timeout/network unavailability -> `503 upstream_unavailable`,
  protocol/provider processing error -> `502 upstream_error`.
- **Rationale**: Сохраняется предсказуемость клиентских ретраев и
  observability без raw upstream payload.
- **Alternatives considered**:
  - Проксировать raw upstream error body: риск утечки.
  - Сводить все ошибки в `500`: теряется диагностическая точность.

## Decision 7: Output sanitization применять рекурсивно только к string leaves

- **Decision**: Tool output обрабатывается JSON-safe traversal: изменяются только
  строковые листья, структура объекта/массива сохраняется.
- **Rationale**: Это основной инвариант Pokrov для корректной совместимости с
  клиентами и MCP server contracts.
- **Alternatives considered**:
  - Строковая сериализация всего output и повторный parse: выше риск
    повреждения формы JSON.
  - Санитизация только top-level string fields: оставляет leak path.

## Decision 8: Metadata-only аудитировать каждый MCP flow

- **Decision**: Писать `McpAuditEvent` с `request_id`, `server`, `tool`,
  `profile`, `final_action`, `rule_hits_total`, `blocked`, `upstream_status`,
  `duration_ms`.
- **Rationale**: Достаточно для forensic/operational анализа и не нарушает
  privacy constraints.
- **Alternatives considered**:
  - Логировать sample аргументы и output snippets: запрещено.
  - Логировать только `blocked=true/false`: недостаточно для диагностики.

## Decision 9: Readiness учитывать валидность MCP config и allowlist ссылок

- **Decision**: `/ready` считается PASS только при валидной MCP секции,
  уникальных `server.id`, непротиворечивых tool policies и корректных secret refs.
- **Rationale**: Некорректный allowlist/policy config напрямую влияет на безопасное
  поведение MCP path.
- **Alternatives considered**:
  - Игнорировать MCP config в readiness: риск запуска в unsafe режиме.
  - Валидировать только синтаксис YAML без семантики: недостаточно.

## Decision 10: Performance-neutral mediation path без тяжелых операций в hot path

- **Decision**: Резолв policy/config структур при старте, в request path избегать
  лишних аллокаций и не добавлять per-argument heavy logging.
- **Rationale**: Нужно удержать p95 <= 50 мс overhead для типового tool call.
- **Alternatives considered**:
  - Пересборка policy map на каждый запрос: неоправданный overhead.
  - Детальный debug-log каждого аргумента: latency + leakage risk.
