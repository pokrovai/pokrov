# Data Model: Codex Responses Compatibility

## CodexResponseRequest

- **Purpose**: Каноническое представление входного `POST /v1/responses` payload до mapping во внутренний LLM flow.
- **Fields**:
  - `model: String`
  - `input: Value` (string/array/object subset, поддерживаемый compatibility layer)
  - `stream: bool`
  - `metadata: Option<Map<String, String>>`
  - `request_id: String`
- **Validation rules**:
  - `model` non-empty;
  - `input` обязан принадлежать поддерживаемому minimal subset;
  - неподдерживаемые поля не должны приводить к raw leakage в error paths.

## CodexToLlmMappingResult

- **Purpose**: Результат детерминированного преобразования `responses` запроса в internal chat-completions shape.
- **Fields**:
  - `normalized_payload: serde_json::Value`
  - `stream_mode: bool`
  - `mapping_warnings: Vec<String>`
- **Validation rules**:
  - для одинакового input/config выдаётся идентичный normalized payload;
  - warnings остаются metadata-only и не включают raw sensitive fragments.

## ResponsesCompatibilityOutcome

- **Purpose**: Итог выполнения compatibility endpoint (allow/block/error) с операционными метаданными.
- **Fields**:
  - `request_id: String`
  - `route: "/v1/responses"`
  - `profile_id: String`
  - `sanitized_input: bool`
  - `sanitized_output: bool`
  - `final_action: "allow" | "mask" | "replace" | "redact" | "block"`
  - `rule_hits_total: u32`
- **Validation rules**:
  - значения action/rule_hits совпадают с фактическим policy decision;
  - не содержит raw payload/data fragments.

## CodexResponseStreamEvent

- **Purpose**: Нормализованная единица stream-ответа в SSE режиме после output sanitization.
- **Fields**:
  - `event_type: "data" | "done"`
  - `data: String`
  - `request_id: String`
- **Validation rules**:
  - `done` завершает поток ровно один раз;
  - JSON events в `data` остаются структурно валидными после sanitization;
  - malformed upstream event обрабатывается predictable metadata-only ошибкой.

## GatewayAuthContext

- **Purpose**: Контекст авторизации доступа клиента к Pokrov boundary.
- **Fields**:
  - `mechanism: "api_key" | "bearer"`
  - `authenticated: bool`
  - `gateway_subject_fingerprint: Option<String>`
  - `failure_code: Option<String>`
- **Validation rules**:
  - при `authenticated=false` upstream вызов не выполняется;
  - `failure_code` соответствует fixed error taxonomy.

## UpstreamCredentialContext

- **Purpose**: Контекст upstream credential resolution для passthrough/static режима.
- **Fields**:
  - `auth_mode: "static" | "passthrough"`
  - `credential_source: "config" | "request"`
  - `credential_present: bool`
  - `failure_code: Option<String>`
- **Validation rules**:
  - `passthrough` требует credential из request;
  - `static` использует credential из config;
  - отсутствие credential в активном режиме приводит к structured metadata-only error.

## CompatibilityAuditSummary

- **Purpose**: Metadata-only аудит результата `responses` запроса.
- **Fields**:
  - `request_id: String`
  - `auth_mode: "static" | "passthrough"`
  - `endpoint: "/v1/responses"`
  - `gateway_auth_result: "pass" | "fail"`
  - `upstream_credential_result: "pass" | "fail"`
  - `decision: "allowed" | "blocked" | "upstream_error"`
  - `rule_hits_total: u32`
- **Validation rules**:
  - только low-cardinality enum values;
  - credentials/payload не попадают в событие.
