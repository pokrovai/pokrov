# Data Model: MCP Mediation

## McpToolCallRequest

- **Purpose**: Нормализованный вход mediation endpoint перед policy checks.
- **Fields**:
  - `request_id: String`
  - `server: String`
  - `tool: String`
  - `arguments: serde_json::Value`
  - `metadata: McpRequestMetadata`
- **Validation rules**:
  - `server` и `tool` не пустые;
  - `arguments` должен быть JSON object;
  - `metadata.profile`, если передан, должен быть allowlisted profile id.

## McpRequestMetadata

- **Purpose**: Контекст запроса без raw payload логирования.
- **Fields**:
  - `agent_id: Option<String>`
  - `profile: Option<String>`
  - `tags: BTreeMap<String, String>`
- **Validation rules**:
  - значения ограничены по длине;
  - metadata не используется для передачи raw arguments/output fragments.

## McpServerDefinition

- **Purpose**: Конфигурация allowlisted MCP server.
- **Fields**:
  - `id: String`
  - `endpoint: String`
  - `enabled: bool`
  - `allowed_tools: Vec<String>`
  - `blocked_tools: Vec<String>`
  - `tools: BTreeMap<String, McpToolPolicy>`
- **Validation rules**:
  - `id` уникален;
  - `endpoint` валиден как `http/https` URL;
  - tool не может одновременно нарушать global safety constraints и быть silently allowed.

## McpToolPolicy

- **Purpose**: Правила валидации и output-sanitization для конкретного tool.
- **Fields**:
  - `tool: String`
  - `enabled: bool`
  - `argument_schema: Option<serde_json::Value>`
  - `argument_constraints: ToolArgumentConstraints`
  - `output_sanitization: Option<bool>`
- **Validation rules**:
  - `tool` совпадает с ключом мапы tools;
  - `argument_schema`, если задана, должна быть валидной JSON Schema;
  - при `enabled=false` tool считается блокируемым.

## ToolArgumentConstraints

- **Purpose**: Дополнительные policy-ограничения над аргументами.
- **Fields**:
  - `max_depth: Option<u8>`
  - `max_string_length: Option<usize>`
  - `required_keys: Vec<String>`
  - `forbidden_keys: Vec<String>`
  - `allowed_path_prefixes: Vec<String>`
- **Validation rules**:
  - лимиты должны быть положительными;
  - `required_keys` и `forbidden_keys` не должны конфликтовать;
  - пустые ограничения не отключают базовые allowlist checks.

## ToolValidationResult

- **Purpose**: Результат schema + policy validation аргументов tool call.
- **Fields**:
  - `valid: bool`
  - `stage: "schema" | "constraints"`
  - `violations: Vec<ValidationViolation>`
- **Validation rules**:
  - при `valid=true` список `violations` пуст;
  - violation entries не содержат raw argument values.

## ValidationViolation

- **Purpose**: Metadata-only описание конкретного нарушения.
- **Fields**:
  - `code: String`
  - `path: String`
  - `message: String`
- **Validation rules**:
  - `message` ограничено по длине;
  - `path` описывает JSON path без вывода значения поля.

## McpToolPolicyDecision

- **Purpose**: Итог policy enforcement перед upstream execution.
- **Fields**:
  - `profile_id: String`
  - `allowed: bool`
  - `final_action: "allow" | "mask" | "replace" | "redact" | "block"`
  - `reason: "server_not_allowlisted" | "tool_not_allowlisted" | "tool_blocklisted" | "argument_invalid" | "allowed"`
  - `rule_hits_total: u32`
- **Validation rules**:
  - `allowed=false` только при `final_action=block`;
  - `reason=allowed` только при `allowed=true`.

## McpUpstreamRequestContext

- **Purpose**: Контекст безопасного upstream вызова для разрешенного tool call.
- **Fields**:
  - `request_id: String`
  - `server_id: String`
  - `tool_id: String`
  - `endpoint: String`
  - `timeout_ms: u64`
- **Validation rules**:
  - формируется только при `allowed=true`;
  - не содержит raw arguments в metadata.

## McpToolResultEnvelope

- **Purpose**: Нормализованный ответ tool execution до/после sanitization.
- **Fields**:
  - `content: serde_json::Value`
  - `content_type: Option<String>`
  - `truncated: bool`
- **Validation rules**:
  - `content` сохраняет валидный JSON shape;
  - truncation не меняет тип верхнеуровневого контейнера.

## McpSanitizationResult

- **Purpose**: Итог обработки tool output санитизатором.
- **Fields**:
  - `sanitized: bool`
  - `action: "allow" | "mask" | "replace" | "redact" | "block"`
  - `rule_hits_total: u32`
  - `safe_output: serde_json::Value`
- **Validation rules**:
  - модифицируются только string leaves;
  - `safe_output` не содержит сырые чувствительные фрагменты из обнаруженных матчей.

## McpResponseMetadata

- **Purpose**: Explain summary в успешном ответе клиенту.
- **Fields**:
  - `profile: String`
  - `action: String`
  - `rule_hits: u32`
  - `sanitized: bool`
- **Validation rules**:
  - metadata-only, без payload fragments;
  - согласована с `McpToolPolicyDecision` и `McpSanitizationResult`.

## McpAuditEvent

- **Purpose**: Metadata-only аудит завершенного MCP mediation flow.
- **Fields**:
  - `request_id: String`
  - `server_id: String`
  - `tool_id: String`
  - `profile_id: String`
  - `final_action: String`
  - `rule_hits_total: u32`
  - `blocked: bool`
  - `upstream_status: Option<u16>`
  - `duration_ms: u64`
- **Validation rules**:
  - при `blocked=true` upstream status может отсутствовать;
  - raw arguments/output/detection fragments запрещены.

## McpMediationError

- **Purpose**: Структурированная ошибка mediation endpoint.
- **Fields**:
  - `request_id: String`
  - `allowed: bool`
  - `error.code: "invalid_request" | "unauthorized" | "tool_call_blocked" | "argument_validation_failed" | "upstream_error" | "upstream_unavailable"`
  - `error.message: String`
  - `error.details.server: Option<String>`
  - `error.details.tool: Option<String>`
  - `error.details.reason: Option<String>`
- **Validation rules**:
  - `allowed` всегда `false` в ошибке;
  - `message` и `details` не включают значения аргументов и tool output.
