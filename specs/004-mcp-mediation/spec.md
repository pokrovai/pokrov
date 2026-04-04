# Спецификация фичи: MCP Mediation

**Ветка фичи**: `004-mcp-mediation`  
**Дата создания**: 2026-04-03  
**Статус**: Draft  
**Вход**: Описание пользователя: "Добавить MCP mediation слой с allowlist server/tool, argument validation, block path и sanitization tool outputs."

## Пользовательские сценарии и тестирование *(обязательно)*

### Пользовательская история 1 - Разрешенный вызов approved tool (Приоритет: P1)

Как platform team, я хочу проксировать разрешенный MCP tool call через Pokrov.AI, чтобы агент использовал approved servers и tools под контролем политики.

**Почему этот приоритет**: MCP mediation является вторым основным interaction path продукта v1.

**Независимая проверка**: Отправить tool call к allowlisted server/tool и убедиться, что разрешенный вызов выполняется, а результат санитизируется до возврата агенту.

**Сценарии приемки**:

1. **Given** allowlisted server и tool с корректными аргументами, **When** клиент вызывает mediation endpoint, **Then** вызов передается upstream и sanitized result возвращается агенту.
2. **Given** tool output содержит чувствительный фрагмент, **When** output sanitization включена, **Then** агент получает безопасно отредактированный результат.

---

### Пользовательская история 2 - Блокировка запрещенного вызова (Приоритет: P2)

Как security stakeholder, я хочу блокировать запрещенные MCP tools или опасные аргументы, чтобы unsafe interaction не доходил до upstream server.

**Почему этот приоритет**: Контроль tool access является ключевой защитой продукта.

**Независимая проверка**: Передать tool call к неразрешенному server/tool или с запрещенным аргументом и убедиться, что сервис возвращает структурированную block error без выполнения вызова.

**Сценарии приемки**:

1. **Given** server не включен в allowlist, **When** клиент выполняет tool call, **Then** сервис возвращает block error и не выполняет upstream call.
2. **Given** tool разрешен, но аргументы нарушают policy, **When** валидация выполняется, **Then** клиент получает структурированную ошибку блокировки.

---

### Пользовательская история 3 - Минимальный pilot subset MCP transport (Приоритет: P3)

Как инженер внедрения, я хочу использовать практический subset MCP interactions, чтобы запустить пилот без поддержки всех transport patterns.

**Почему этот приоритет**: PRD ограничивает v1 минимальным рабочим subset, а не полным MCP coverage.

**Независимая проверка**: Выполнить пилотный набор MCP tool interactions через утвержденный mediation endpoint.

**Сценарии приемки**:

1. **Given** pilot-compatible MCP request, **When** mediation layer обрабатывает его, **Then** сервис обеспечивает allowlist, validation, sanitization и audit.
2. **Given** неподдерживаемый transport variant, **When** клиент пытается его использовать, **Then** сервис возвращает предсказуемое отклонение в рамках объявленного scope.

### Edge Cases

- Что происходит, если server allowlisted, но конкретный tool явно заблокирован?
- Как система ведет себя, если upstream MCP server недоступен?
- Что происходит, если схема аргументов отсутствует, но policy constraints все равно должны применяться?

## Требования *(обязательно)*

### Функциональные требования

- **FR-001**: Система MUST принимать MCP mediation запросы через объявленный v1 endpoint.
- **FR-002**: Система MUST аутентифицировать доступ к MCP mediation endpoint по API key.
- **FR-003**: Система MUST проверять allowlist для целевого MCP server до любого upstream interaction.
- **FR-004**: Система MUST проверять allowlist/blocklist для вызываемого tool.
- **FR-005**: Система MUST валидировать tool arguments по доступной схеме и policy constraints.
- **FR-006**: Система MUST блокировать вызов и возвращать структурированную ошибку, если server/tool/arguments нарушают policy.
- **FR-007**: Система MUST выполнять output sanitization для tool result до возврата агенту.
- **FR-008**: Система MUST поддерживать pilot-ready subset MCP interactions, достаточный для целевых сценариев v1.
- **FR-009**: Система MUST формировать metadata-only audit event для каждого MCP flow.
- **FR-010**: Система MUST объяснять block outcome через безопасный summary без раскрытия чувствительных значений аргументов или outputs.
- **FR-011**: Pilot-ready subset для v1 ограничен HTTP JSON mediation endpoint с синхронным tool invocation; stateful session transports, bi-directional streaming и long-lived MCP channels являются out-of-scope.
- **FR-012**: Если часть аргументов проходит схему, но итоговое policy решение = deny/block, система MUST возвращать единый block outcome без upstream retry; стратегия автоматического retry на клиенте не навязывается и документируется как external responsibility.

### Ключевые сущности *(добавляйте, если фича работает с данными)*

- **ToolCallRequest**: Запрос на выполнение MCP tool с server, tool, arguments и metadata.
- **ToolPolicy**: Правила allowlist/blocklist и ограничения на аргументы.
- **ToolValidationResult**: Итог проверки схемы и policy constraints для аргументов.
- **MCPAuditEvent**: Метаданные завершенного или заблокированного MCP flow.

## Ограничения безопасности и приватности *(обязательно)*

- Аргументы tool call и tool outputs MUST NOT логироваться в сыром виде по умолчанию.
- Block path MUST завершаться до upstream execution.
- Explain и audit outputs MUST содержать только metadata summary по server/tool/policy outcome.

## Операционная готовность *(обязательно для runtime-изменений)*

- **Логи**: MUST фиксироваться server id, tool id, policy action и исход выполнения без raw arguments/output.
- **Метрики**: MUST учитываться общее число tool calls, blocked calls и latency MCP path.
- **Health/Readiness**: Readiness MUST учитывать валидность конфигурации approved MCP servers.
- **Документация/конфиг**: MUST быть описаны allowlist semantics, argument validation rules и scope границы MCP subset.

## Required Test Coverage *(обязательно)*

- **Unit**: Allowlist checks, blocklist precedence, argument validation, block response formatting.
- **Integration**: Allowed tool path, blocked tool path, blocked arguments path, sanitized output path, upstream unavailability path.
- **Performance**: Проверка latency overhead на типовом pilot MCP call.
- **Security**: Проверка invalid API key, отсутствия raw arguments/output в логах и корректного block-before-execution behavior.

## Success Criteria *(обязательно)*

### Измеримые результаты

- **SC-001**: 100% вызовов к неallowlisted server или blocked tool завершаются без upstream execution.
- **SC-002**: 100% проверенных MCP audit/log артефактов не содержат raw arguments и raw tool outputs.
- **SC-003**: Не менее 95% типовых разрешенных MCP вызовов укладываются в целевой latency overhead v1.
- **SC-004**: Pilot subset покрывает все целевые MCP сценарии, описанные для v1, без включения out-of-scope transport functionality.

## Acceptance Evidence *(обязательно)*

- Интеграционные тесты allowed path, blocked path, argument validation и output sanitization.
- Проверка структурированных block responses и upstream failure responses.
- Конфигурационная документация по allowlist/blocklist.
- Лог- и audit-сэмплы без sensitive content.

## Assumptions

- v1 ограничивается одним практическим mediation surface, а не полной реализацией всех MCP transport patterns.
- Approved server and tool policies задаются статически через конфиг v1.
- Sanitization core и bootstrap runtime уже доступны к моменту реализации этой фичи.
- Предполагается, что upstream MCP server публикует стабильные схемы аргументов для in-scope tools; при нарушении схемы применяется deterministic block/error contract без деградации metadata-only безопасности.
